use std::{
    collections::HashMap,
    io::{BufReader, Write},
    ops::Range,
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

use elf::endian::LittleEndian;
use framehop::{
    x86_64::{CacheX86_64, UnwindRegsX86_64, UnwinderX86_64},
    ExplicitModuleSectionInfo, Module, Unwinder,
};
use qapi::{qmp, Qmp, Stream};

use crate::{args::Profiler, userspace::userspace_output_path, GlobalMeta};

fn parse_elf<P: AsRef<Path>>(path: P) -> anyhow::Result<(ElfSymbolsCache, Module<Vec<u8>>)> {
    let file_data = std::fs::read(path.as_ref())?.into_boxed_slice();
    let file_data_slice = &file_data[..];

    let elf_file = elf::ElfBytes::<LittleEndian>::minimal_parse(file_data_slice)
        .map_err(|e| anyhow::anyhow!("Failed to parse ELF file: {}", e))?;

    let text_section = elf_file
        .section_header_by_name(".text")?
        .ok_or_else(|| anyhow::anyhow!("ELF file does not contain a .text section"))?;
    let eh_frame_section = elf_file
        .section_header_by_name(".eh_frame")?
        .ok_or_else(|| anyhow::anyhow!("ELF file does not contain a .eh_frame section"))?;

    println!(
        "Found .text section at {:#x} with size {}",
        text_section.sh_addr, text_section.sh_size
    );
    println!(
        "Found .eh_frame section at {:#x} with size {}",
        eh_frame_section.sh_addr, eh_frame_section.sh_size
    );

    let mut vma_base = 0xFFFFFFFFFFFFFFFF;
    let mut vma_end = 0;

    for section in elf_file
        .section_headers()
        .ok_or_else(|| anyhow::anyhow!("Failed to read section headers"))?
    {
        if section.sh_addr < vma_base && section.sh_size > 0 && section.sh_addr != 0 {
            vma_base = section.sh_addr;
        }
        let section_end = section.sh_addr + section.sh_size;
        if section_end > vma_end {
            vma_end = section_end;
        }
    }

    println!("VMA range: {:#x}..{:#x}", vma_base, vma_end);

    let module = Module::new(
        path.as_ref().to_string_lossy().to_string(),
        vma_base..vma_end,
        vma_base,
        ExplicitModuleSectionInfo {
            base_svma: text_section.sh_addr,
            text_svma: Some(text_section.sh_addr..(text_section.sh_addr + text_section.sh_size)),
            text: Some(
                file_data_slice[text_section.sh_offset as usize
                    ..(text_section.sh_offset + text_section.sh_size) as usize]
                    .to_vec(),
            ),
            eh_frame_svma: Some(
                eh_frame_section.sh_addr..(eh_frame_section.sh_addr + eh_frame_section.sh_size),
            ),
            eh_frame: Some(
                file_data_slice[eh_frame_section.sh_offset as usize
                    ..(eh_frame_section.sh_offset + eh_frame_section.sh_size) as usize]
                    .to_vec(),
            ),
            ..Default::default()
        },
    );

    // Initialize symbols
    let mut symbols = Vec::new();
    if let Some(symbol_table) = elf_file.symbol_table()? {
        for symbol in symbol_table.0.iter() {
            if symbol.st_shndx != 0 && symbol.st_size > 0 {
                let start_addr = symbol.st_value;
                let end_addr = start_addr + symbol.st_size;

                let name = symbol_table.1.get(symbol.st_name as usize)?;
                symbols.push(SymbolCacheEntry {
                    address_range: start_addr..end_addr,
                    symbol: rustc_demangle::demangle(name).to_string(),
                });
            }
        }
    }

    symbols.sort_by_key(|entry| entry.address_range.start);

    Ok((
        ElfSymbolsCache {
            symbols,
            vma_range: vma_base..vma_end,
        },
        module,
    ))
}

#[derive(Debug)]
struct SymbolCacheEntry {
    address_range: Range<u64>,
    symbol: String,
}

struct ElfSymbolsCache {
    symbols: Vec<SymbolCacheEntry>,
    vma_range: Range<u64>,
}

impl ElfSymbolsCache {
    pub fn contains(&self, address: u64) -> bool {
        self.vma_range.contains(&address)
    }

    pub fn get(&self, address: u64) -> Option<&SymbolCacheEntry> {
        if !self.contains(address) {
            return None;
        }

        self.symbols
            .iter()
            .find(|entry| entry.address_range.contains(&address))
    }
}

struct StackUnwinder {
    symbols: Vec<ElfSymbolsCache>,
    unwinder: UnwinderX86_64<Vec<u8>>,
    cache: CacheX86_64,
}

impl StackUnwinder {
    pub fn new<P: AsRef<Path>>(kernel_file: P) -> anyhow::Result<Self> {
        let (symbols, module) = parse_elf(kernel_file)?;

        let cache = CacheX86_64::new();
        let mut unwinder: UnwinderX86_64<Vec<u8>> = UnwinderX86_64::new();
        unwinder.add_module(module);

        Ok(StackUnwinder {
            symbols: vec![symbols],
            unwinder,
            cache,
        })
    }

    pub fn add_userspace_module<P: AsRef<Path>>(
        &mut self,
        userspace_file: P,
    ) -> anyhow::Result<()> {
        let (symbols, module) = parse_elf(userspace_file)?;
        self.unwinder.add_module(module);
        self.symbols.push(symbols);
        Ok(())
    }

    pub fn unwind_stack<F>(&mut self, rip: u64, rsp: u64, rbp: u64, read_stack: &mut F) -> Vec<u64>
    where
        F: FnMut(u64) -> Result<u64, ()>,
    {
        let regs = UnwindRegsX86_64::new(rip, rsp, rbp);
        let mut iter = self
            .unwinder
            .iter_frames(rip, regs, &mut self.cache, read_stack);

        let mut frames = Vec::new();
        while let Ok(Some(frame)) = iter.next() {
            frames.push(frame.address());
        }
        frames
    }

    pub fn resolve_symbol(&self, address: u64) -> anyhow::Result<String> {
        for symbols in &self.symbols {
            if let Some(entry) = symbols.get(address) {
                return Ok(entry.symbol.clone());
            }
        }

        Ok(format!("<unknown:0x{:x}>", address))
    }
}

struct Sampler<'a> {
    qmp: Qmp<Stream<BufReader<&'a UnixStream>, &'a UnixStream>>,
    unwinder: StackUnwinder,
}

fn hex_to_u64(hex_str: &str) -> anyhow::Result<u64> {
    u64::from_str_radix(hex_str.trim().trim_start_matches("0x"), 16)
        .map_err(|e| anyhow::anyhow!("Failed to parse hex string '{}': {}", hex_str, e))
}

impl<'a> Sampler<'a> {
    pub fn new(
        qmp: Qmp<Stream<BufReader<&'a UnixStream>, &'a UnixStream>>,
        unwinder: StackUnwinder,
    ) -> Self {
        Sampler { qmp, unwinder }
    }

    pub fn get_registers(&mut self, verbose: bool) -> anyhow::Result<(u64, u64, u64)> {
        let regs = self.qmp.execute(&qmp::human_monitor_command {
            cpu_index: Some(0),
            command_line: format!("info registers"),
        })?;

        if verbose {
            println!("Registers: {}", regs);
        }

        let get_reg = |name: &str| -> anyhow::Result<u64> {
            hex_to_u64(
                regs.split_once(name)
                    .ok_or(anyhow::anyhow!("Failed to find {name} in registers"))?
                    .1
                    .split_ascii_whitespace()
                    .next()
                    .ok_or(anyhow::anyhow!("Failed to parse {name} value"))?,
            )
        };

        let rip = get_reg("RIP=")?;
        let rsp = get_reg("RSP=")?;
        let rbp = get_reg("RBP=")?;

        Ok((rip, rsp, rbp))
    }

    pub fn get_stack_symbols(
        &self,
        stack: &[u64],
        show_addresses: bool,
    ) -> anyhow::Result<Vec<String>> {
        let mut symbols = Vec::new();

        for &address in stack {
            let symbol = self.unwinder.resolve_symbol(address)?;

            if show_addresses {
                symbols.push(format!("{} [0x{:x}]", symbol, address));
            } else {
                symbols.push(symbol);
            }
        }

        Ok(symbols)
    }

    pub fn sample_with_timing(&mut self, verbose: bool) -> anyhow::Result<(Vec<u64>, Duration)> {
        let start_time = Instant::now();

        self.qmp.execute(&qmp::stop {})?;

        let (rip, rsp, rbp) = self.get_registers(verbose)?;

        let result = self.unwinder.unwind_stack(rip, rsp, rbp, &mut |addr| {
            let register_read = self
                .qmp
                .execute(&qmp::human_monitor_command {
                    cpu_index: Some(0),
                    command_line: format!("x/1gx {:#x}", addr),
                })
                .map_err(|_| ())?;

            let value = register_read.split(": ").nth(1).ok_or(())?;

            hex_to_u64(value).map_err(|_| ())
        });

        self.qmp.execute(&qmp::cont {})?;
        let total_time = start_time.elapsed();

        Ok((result, total_time))
    }
}

pub fn run(meta: &GlobalMeta, args: &Profiler) -> anyhow::Result<()> {
    let kernel_path = meta
        .target_path
        .join(meta.profile_path())
        .join("iso")
        .join("boot")
        .join("kernel");
    assert!(kernel_path.exists(), "Kernel ELF file does not exist");

    let mut unwinder = StackUnwinder::new(&kernel_path)?;

    if let Some(user_program_name) = &args.user_program {
        let user_program_path = userspace_output_path(meta, user_program_name);

        if user_program_name.is_empty() || !user_program_path.exists() {
            return Err(anyhow::anyhow!(
                "Userspace program '{user_program_name}' does not exist in '{}'",
                user_program_path.display()
            ));
        }

        unwinder.add_userspace_module(user_program_path)?;
    }

    let socket = UnixStream::connect(args.qmp_socket.as_deref().unwrap_or("./qmp-socket"))
        .map_err(|e| anyhow::anyhow!("Failed to connect to socket: {}", e))?;

    let mut qmp = qapi::Qmp::from_stream(&socket);

    let info = qmp.handshake()?;
    if args.verbose {
        println!("QMP Info: {:?}", info);
    }

    let mut sampler = Sampler::new(qmp, unwinder);

    // Handle single sample case (when duration is 0 or very short)
    if args.duration_sec < 1 || args.one_shot {
        let (stack, took) = sampler.sample_with_timing(args.verbose)?;
        let symbols = sampler.get_stack_symbols(&stack, args.show_addresses)?;

        println!("Current stack trace (took {took:?}):");
        for (i, symbol) in symbols.iter().enumerate() {
            println!("{:>3}: {}", i, symbol);
        }
        return Ok(());
    }

    // Continuous or timed sampling
    let mut sample_counts: HashMap<String, usize> = HashMap::new();
    let mut total_samples = 0;
    let mut failed_samples = 0;
    let mut total_sample_time = Duration::ZERO;
    let mut total_symbol_time = Duration::ZERO;

    let interval_duration = Duration::from_millis(args.interval_ms);
    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(args.duration_sec);

    println!("Starting stack sampling...");
    if args.verbose {
        println!("Interval: {}ms", args.interval_ms);
        println!("Duration: {}s", args.duration_sec);
    }

    loop {
        if Instant::now() >= end_time {
            if args.verbose {
                println!("Reached time limit");
            }
            break;
        }

        let interval_start = Instant::now();

        // Take a sample
        match sampler.sample_with_timing(args.verbose) {
            Ok((stack, sample_time)) => {
                total_sample_time += sample_time;

                if !stack.is_empty() {
                    let symbol_start = Instant::now();
                    match sampler.get_stack_symbols(&stack, false) {
                        Ok(mut symbols) => {
                            // Create folded stack format (root to leaf)
                            symbols.reverse();
                            let folded_stack = symbols.join(";");
                            *sample_counts.entry(folded_stack).or_insert(0) += 1;
                            total_samples += 1;

                            if args.verbose {
                                println!("Sample {}: {} frames", total_samples, stack.len());
                            }
                        }
                        Err(e) => {
                            failed_samples += 1;
                            if args.verbose {
                                println!("Failed to resolve symbols: {}", e);
                            }
                        }
                    }
                    total_symbol_time += Instant::now() - symbol_start;
                } else {
                    failed_samples += 1;
                    if args.verbose {
                        println!("Empty stack trace");
                    }
                }
            }
            Err(e) => {
                failed_samples += 1;
                if args.verbose {
                    println!("Failed to sample stack: {}", e);
                }
            }
        }

        // Sleep for the remaining interval time
        let elapsed = interval_start.elapsed();
        if elapsed < interval_duration {
            std::thread::sleep(interval_duration - elapsed);
        } else if args.verbose {
            println!(
                "Warning: Sampling took longer than interval ({:?} > {:?})",
                elapsed, interval_duration
            );
        }
    }

    let total_elapsed = start_time.elapsed();

    // Print statistics
    println!("\n=== Sampling Statistics ===");
    println!("Total samples: {}", total_samples);
    println!("Failed samples: {}", failed_samples);
    println!("Unique stacks: {}", sample_counts.len());
    println!("Total time: {:?}", total_elapsed);
    if total_samples > 0 {
        println!(
            "Average sample time: {:?}",
            total_sample_time / total_samples as u32
        );
        println!("Total sample time: {:?}", total_sample_time);
        println!(
            "Average symbol resolution time: {:?}",
            total_symbol_time / total_samples as u32
        );
        println!("Total symbol resolution time: {:?}", total_symbol_time);
        println!(
            "Success rate: {:.1}%",
            (total_samples as f64 / (total_samples + failed_samples) as f64) * 100.0
        );
    }

    // Write output file if specified
    if let Some(output_path) = &args.output {
        println!("Writing {} samples to {}", sample_counts.len(), output_path);

        let mut file = std::fs::File::create(output_path)?;
        for (stack, count) in sample_counts.iter() {
            writeln!(file, "{} {}", stack, count)?;
        }

        println!("Folded stack samples written to: {}", output_path);
        println!(
            "Generate flamegraph with: flamegraph.pl {} > flamegraph.svg",
            output_path
        );
    } else {
        // Print top stacks
        let mut sorted_stacks: Vec<_> = sample_counts.iter().collect();
        sorted_stacks.sort_by(|a, b| b.1.cmp(a.1));

        println!("\n=== Top Stack Traces ===");
        for (i, (stack, count)) in sorted_stacks.iter().take(10).enumerate() {
            let percentage = (**count as f64 / total_samples as f64) * 100.0;
            println!(
                "{:>2}. ({:>3} samples, {:>5.1}%) {}",
                i + 1,
                count,
                percentage,
                stack
            );
        }
    }

    Ok(())
}
