use std::{
    collections::{HashMap, HashSet},
    io::{BufReader, Write},
    ops::Range,
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::Context;
use elf::endian::LittleEndian;
use framehop::{
    ExplicitModuleSectionInfo, Module, Unwinder,
    x86_64::{CacheX86_64, UnwindRegsX86_64, UnwinderX86_64},
};
use qapi::{Qmp, Stream, qmp};

use crate::{GlobalMeta, args::Profiler, userspace::userspace_output_path};

const KERNEL_START: u64 = 0xFFFFFFFF80000000;

fn get_elf_symbols(
    elf_file: &elf::ElfBytes<LittleEndian>,
) -> anyhow::Result<Vec<SymbolCacheEntry>> {
    // Initialize symbols
    let mut symbols = Vec::new();
    if let Some(symbol_table) = elf_file.symbol_table()? {
        for symbol in symbol_table.0.iter() {
            if symbol.st_shndx != 0 {
                let start_addr = symbol.st_value;
                let end_addr = start_addr
                    + if symbol.st_size == 0 {
                        1 // Ensure at least one byte for symbols with zero size
                    } else {
                        symbol.st_size
                    };

                let name = symbol_table.1.get(symbol.st_name as usize)?;
                symbols.push(SymbolCacheEntry {
                    address_range: start_addr..end_addr,
                    symbol: rustc_demangle::demangle(name).to_string(),
                });
            }
        }
    }

    symbols.sort_by_key(|entry| entry.address_range.start);

    Ok(symbols)
}

fn parse_elf<P: AsRef<Path>>(path: P) -> anyhow::Result<(ModuleSymbols, Module<&'static [u8]>)> {
    let file_data = std::fs::read(path.as_ref())?.into_boxed_slice();
    let file_data_slice = Box::leak(file_data);

    let elf_file = elf::ElfBytes::<LittleEndian>::minimal_parse(file_data_slice)
        .map_err(|e| anyhow::anyhow!("Failed to parse ELF file: {}", e))?;

    let text_section = elf_file
        .section_header_by_name(".text")?
        .ok_or_else(|| anyhow::anyhow!("ELF file does not contain a .text section"))?;
    let eh_frame_section = elf_file
        .section_header_by_name(".eh_frame")?
        .ok_or_else(|| anyhow::anyhow!("ELF file does not contain a .eh_frame section"))?;

    println!(
        "Found .text section at {:#x} with size {:#x}",
        text_section.sh_addr, text_section.sh_size
    );
    println!(
        "Found .eh_frame section at {:#x} with size {:#x}",
        eh_frame_section.sh_addr, eh_frame_section.sh_size
    );

    let mut vma_base = 0xFFFFFFFFFFFFFFFF;
    let mut vma_end = 0;

    for section in elf_file
        .segments()
        .ok_or_else(|| anyhow::anyhow!("Failed to read section headers"))?
    {
        if section.p_vaddr < vma_base && section.p_memsz > 0 && section.p_vaddr != 0 {
            vma_base = section.p_vaddr;
        }
        let section_end = section.p_vaddr + section.p_memsz;
        if section_end > vma_end {
            vma_end = section_end;
        }
    }

    println!("VMA range: {vma_base:#x}..{vma_end:#x}");

    let module = Module::new(
        path.as_ref().to_string_lossy().to_string(),
        vma_base..vma_end,
        vma_base,
        ExplicitModuleSectionInfo {
            base_svma: vma_base,
            text_svma: Some(text_section.sh_addr..(text_section.sh_addr + text_section.sh_size)),
            text: Some(
                &file_data_slice[text_section.sh_offset as usize
                    ..(text_section.sh_offset + text_section.sh_size) as usize],
            ),
            eh_frame_svma: Some(
                eh_frame_section.sh_addr..(eh_frame_section.sh_addr + eh_frame_section.sh_size),
            ),
            eh_frame: Some(
                &file_data_slice[eh_frame_section.sh_offset as usize
                    ..(eh_frame_section.sh_offset + eh_frame_section.sh_size) as usize],
            ),
            ..Default::default()
        },
    );

    Ok((
        ModuleSymbols {
            symbols: get_elf_symbols(&elf_file)?,
            vma_range: vma_base..vma_end,
            pid: None,
        },
        module,
    ))
}

fn parse_userspace_elf<P: AsRef<Path>>(path: P) -> anyhow::Result<(ModuleSymbols, u64)> {
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
        "Found .text section at {:#x} with size {:#x}",
        text_section.sh_addr, text_section.sh_size
    );
    println!(
        "Found .eh_frame section at {:#x} with size {:#x}",
        eh_frame_section.sh_addr, eh_frame_section.sh_size
    );

    let mut vma_base = 0xFFFFFFFFFFFFFFFF;
    let mut vma_end = 0;

    for section in elf_file
        .segments()
        .ok_or_else(|| anyhow::anyhow!("Failed to read section headers"))?
    {
        if section.p_vaddr < vma_base && section.p_memsz > 0 && section.p_vaddr != 0 {
            vma_base = section.p_vaddr;
        }
        let section_end = section.p_vaddr + section.p_memsz;
        if section_end > vma_end {
            vma_end = section_end;
        }
    }

    let text = &file_data_slice
        [text_section.sh_offset as usize..(text_section.sh_offset + text_section.sh_size) as usize];

    Ok((
        ModuleSymbols {
            symbols: get_elf_symbols(&elf_file)?,
            vma_range: vma_base..vma_end,
            pid: None,
        },
        xxhash_rust::xxh3::xxh3_64(text),
    ))
}

fn read_memory_qmp(
    qmp: &mut Qmp<Stream<BufReader<&UnixStream>, &UnixStream>>,
    address: u64,
    size: u64,
) -> anyhow::Result<Vec<u8>> {
    qmp.execute(&qmp::memsave {
        cpu_index: Some(0),
        val: address,
        size,
        filename: "/dev/shm/emerald-profile-memory-dump".to_string(),
    })?;

    let content = std::fs::read("/dev/shm/emerald-profile-memory-dump")
        .map_err(|e| anyhow::anyhow!("Failed to read memory dump: {}", e))?;

    Ok(content)
}

#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
struct ProcessMetadata {
    pub pid: u64,
    pub image_base: usize,
    pub image_size: usize,
    pub program_headers_offset: usize,
    pub eh_frame_address: usize,
    pub eh_frame_size: usize,
    pub text_address: usize,
    pub text_size: usize,
}

#[derive(Debug)]
struct SymbolCacheEntry {
    address_range: Range<u64>,
    symbol: String,
}

#[derive(Debug)]
struct ModuleSymbols {
    symbols: Vec<SymbolCacheEntry>,
    vma_range: Range<u64>,
    pid: Option<u64>,
}

impl ModuleSymbols {
    pub fn get(&self, address: u64, pid: Option<u64>) -> Option<&SymbolCacheEntry> {
        if !self.vma_range.contains(&address) {
            return None;
        }
        if self.pid.is_some() && self.pid != pid {
            return None; // If PID is set, we only match the same PID
        }

        self.symbols.iter().rfind(|entry| {
            // Check if the address is within the range of the symbol (some symbols have zero size, so let's not check the end)
            // we are checking from reverse to find the first symbol that contains the address
            entry.address_range.start <= address
        })
    }
}

struct UserSymbolsCache {
    // based on `.text` section hash
    entries: HashMap<u64, (ModuleSymbols, String)>,
}

impl UserSymbolsCache {
    pub fn load_programs<P: AsRef<Path>>(dir: P) -> anyhow::Result<Self> {
        let mut entries = HashMap::new();
        // parse all ELF files in the directory
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if entry.path().is_file() {
                println!("[+] Loading userspace ELF file: {:?}", entry.path());
                match parse_userspace_elf(entry.path()) {
                    Ok((symbols, text_hash)) => {
                        if symbols.symbols.is_empty() {
                            println!("[-] No symbols found in {:?}, skipping", entry.path());
                            continue;
                        }
                        // Store the symbols with the hash of the .text section as the key
                        if entries
                            .insert(
                                text_hash,
                                (symbols, entry.file_name().to_string_lossy().to_string()),
                            )
                            .is_some()
                        {
                            println!(
                                "Warning: Duplicate symbols for hash {:#x} in {:?}",
                                text_hash,
                                entry.path()
                            );
                        }
                    }
                    Err(e) => {
                        println!("[-] Failed to parse {:?}: {}", entry.path(), e);
                        continue;
                    }
                }
            }
        }

        println!(
            "Loaded {} userspace ELF files into symbols cache",
            entries.len()
        );

        Ok(UserSymbolsCache { entries })
    }

    pub fn take(&mut self, hash: u64) -> Option<(ModuleSymbols, String)> {
        self.entries.remove(&hash)
    }
}

struct StackUnwinder {
    unwinder: UnwinderX86_64<&'static [u8]>,
    cache: CacheX86_64,
}

impl StackUnwinder {
    pub fn new() -> Self {
        let cache = CacheX86_64::new();
        let unwinder = UnwinderX86_64::new();

        StackUnwinder { unwinder, cache }
    }

    fn add_module(&mut self, module: Module<&'static [u8]>) {
        self.unwinder.add_module(module);
    }

    fn detach_userspace_module(&mut self, image_base: u64) {
        self.unwinder.remove_module(image_base);
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
}

struct ProcessState {
    meta: ProcessMetadata,
    module: Module<&'static [u8]>,
    symbols: Option<ModuleSymbols>,
    name: String,
}

struct Sampler<'a> {
    qmp: Qmp<Stream<BufReader<&'a UnixStream>, &'a UnixStream>>,
    unwinder: StackUnwinder,
    processes: HashMap<u64, ProcessState>,
    current_process: Option<u64>,
    current_process_name: String,
    user_programs_cache: UserSymbolsCache,
    kernel_symbols: ModuleSymbols,
}

fn hex_to_u64(hex_str: &str) -> anyhow::Result<u64> {
    u64::from_str_radix(hex_str.trim().trim_start_matches("0x"), 16)
        .map_err(|e| anyhow::anyhow!("Failed to parse hex string '{}': {}", hex_str, e))
}

impl<'a> Sampler<'a> {
    pub fn new<P: AsRef<Path>>(
        qmp: Qmp<Stream<BufReader<&'a UnixStream>, &'a UnixStream>>,
        user_symbols_cache: UserSymbolsCache,
        kernel_path: P,
    ) -> Self {
        let (kernel_symbols, kernel_module) =
            parse_elf(kernel_path).expect("Failed to parse kernel ELF file");
        let mut unwinder = StackUnwinder::new();
        unwinder.add_module(kernel_module);

        Sampler {
            qmp,
            unwinder,
            processes: HashMap::new(),
            current_process: None,
            current_process_name: String::new(),
            user_programs_cache: user_symbols_cache,
            kernel_symbols,
        }
    }

    pub fn get_registers(&mut self, verbose: bool) -> anyhow::Result<(u64, u64, u64)> {
        let regs = self.qmp.execute(&qmp::human_monitor_command {
            cpu_index: Some(0),
            command_line: "info registers".to_string(),
        })?;

        if verbose {
            println!("Registers: {regs}");
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

    pub fn get_process_meta(&mut self) -> anyhow::Result<ProcessMetadata> {
        let meta_read = self.qmp.execute(&qmp::human_monitor_command {
            cpu_index: Some(0),
            command_line: format!("x/10gx {:#x}", 0xFFFF_FF7F_FFFF_E000u64),
        })?;

        // parse the process metadata
        let mut values = meta_read
            .lines()
            .map(|line| {
                line.split_once(':')
                    .map(|s| s.1.trim())
                    .expect("Invalid format")
            })
            .flat_map(|s| s.split_whitespace());

        let mut next_value = |name: &str| -> anyhow::Result<u64> {
            values
                .next()
                .ok_or_else(|| anyhow::anyhow!("Failed to find {name} in process metadata"))
                .map(hex_to_u64)?
        };

        Ok(ProcessMetadata {
            pid: next_value("pid")?,
            image_base: next_value("image_base")? as usize,
            image_size: next_value("image_size")? as usize,
            program_headers_offset: next_value("program_headers_offset")? as usize,
            eh_frame_address: next_value("eh_frame_address")? as usize,
            eh_frame_size: next_value("eh_frame_size")? as usize,
            text_address: next_value("text_address")? as usize,
            text_size: next_value("text_size")? as usize,
        })
    }

    pub fn read_memory(&mut self, address: u64, size: u64) -> anyhow::Result<Vec<u8>> {
        read_memory_qmp(&mut self.qmp, address, size)
            .with_context(|| format!("Failed to read memory at {address:#x} with size {size}"))
    }

    pub fn resolve_symbol(&self, address: u64, pid: Option<u64>) -> anyhow::Result<String> {
        if address >= KERNEL_START {
            // Kernel address
            if let Some(entry) = self.kernel_symbols.get(address, pid) {
                return Ok(entry.symbol.clone());
            }
        } else if let Some(pid) = pid {
            // Userspace address
            if let Some(process) = self.processes.get(&pid)
                && let Some(symbols) = &process.symbols
                && let Some(entry) = symbols.get(address, Some(pid))
            {
                return Ok(entry.symbol.clone());
            }
        }

        Ok(format!("<unknown:0x{address:x}>"))
    }

    pub fn get_stack_symbols(
        &self,
        stack: &[u64],
        show_addresses: bool,
        pid: Option<u64>,
    ) -> anyhow::Result<Vec<String>> {
        let mut symbols = Vec::new();

        for &address in stack {
            let symbol = self.resolve_symbol(address, pid)?;

            if show_addresses {
                symbols.push(format!("{symbol} [0x{address:x}]"));
            } else {
                symbols.push(symbol);
            }
        }

        Ok(symbols)
    }

    pub fn sample_with_timing(
        &mut self,
        args: &Profiler,
    ) -> anyhow::Result<(Vec<u64>, Option<u64>, Duration)> {
        let start_time = Instant::now();

        self.qmp.execute(&qmp::stop {})?;

        let (rip, rsp, rbp) = self.get_registers(args.verbose)?;

        // userspace
        let pid = if rip < KERNEL_START {
            if args.kernel_only {
                self.qmp.execute(&qmp::cont {})?;
                return Ok((Vec::new(), None, start_time.elapsed()));
            }
            let process_meta = self.get_process_meta()?;
            self.set_process(&process_meta)?;
            Some(process_meta.pid)
        } else {
            if args.user_only {
                self.qmp.execute(&qmp::cont {})?;
                return Ok((Vec::new(), None, start_time.elapsed()));
            }
            None
        };

        let result = self.unwinder.unwind_stack(rip, rsp, rbp, &mut |addr| {
            let register_read = self
                .qmp
                .execute(&qmp::human_monitor_command {
                    cpu_index: Some(0),
                    command_line: format!("x/1gx {addr:#x}"),
                })
                .map_err(|_| ())?;

            let value = register_read.split(": ").nth(1).ok_or(())?;

            hex_to_u64(value).map_err(|_| ())
        });

        self.qmp.execute(&qmp::cont {})?;
        let total_time = start_time.elapsed();

        Ok((result, pid, total_time))
    }

    fn set_process(&mut self, process_meta: &ProcessMetadata) -> anyhow::Result<()> {
        if self.current_process == Some(process_meta.pid) {
            return Ok(()); // Process already set
        }

        if !self.processes.contains_key(&process_meta.pid) {
            println!(
                "Adding userspace process: PID={}, Image Base={:#x}, Image Size={:#x}",
                process_meta.pid, process_meta.image_base, process_meta.image_size
            );
            println!(
                "Text: {:#x}..{:#x}, EH Frame: {:#x}..{:#x}",
                process_meta.text_address,
                process_meta.text_address + process_meta.text_size,
                process_meta.eh_frame_address,
                process_meta.eh_frame_address + process_meta.eh_frame_size
            );

            let text = Box::leak(
                self.read_memory(
                    process_meta.text_address as u64,
                    process_meta.text_size as u64,
                )?
                .into_boxed_slice(),
            );
            let eh_frame = Box::leak(
                self.read_memory(
                    process_meta.eh_frame_address as u64,
                    process_meta.eh_frame_size as u64,
                )?
                .into_boxed_slice(),
            );

            let (symbols, program_name) = self
                .user_programs_cache
                .take(xxhash_rust::xxh3::xxh3_64(&text))
                .map(|(symbols, name)| (Some(symbols), name))
                .unwrap_or_else(|| {
                    println!(
                        "No symbols found for userspace process PID={}, using empty symbols",
                        process_meta.pid
                    );
                    (None, "<unknown>".to_string())
                });

            let module = Module::new(
                format!("userspace-{}", process_meta.pid),
                process_meta.image_base as u64
                    ..(process_meta.image_base as u64 + process_meta.image_size as u64),
                process_meta.image_base as u64,
                ExplicitModuleSectionInfo {
                    base_svma: process_meta.image_base as u64,
                    text_svma: Some(
                        process_meta.text_address as u64
                            ..(process_meta.text_address as u64 + process_meta.text_size as u64),
                    ),
                    text: Some(&text[..]),
                    eh_frame_svma: Some(
                        process_meta.eh_frame_address as u64
                            ..(process_meta.eh_frame_address as u64
                                + process_meta.eh_frame_size as u64),
                    ),
                    eh_frame: Some(&eh_frame[..]),
                    ..Default::default()
                },
            );

            self.processes.insert(
                process_meta.pid,
                ProcessState {
                    meta: *process_meta,
                    module: module.clone(),
                    symbols,
                    name: program_name.clone(),
                },
            );
        }

        if let Some(pid) = self.current_process {
            self.unwinder
                .detach_userspace_module(self.processes[&pid].meta.image_base as u64);
        }

        self.unwinder
            .add_module(self.processes[&process_meta.pid].module.clone());

        self.current_process = Some(process_meta.pid);
        self.current_process_name = self.processes[&process_meta.pid].name.clone();
        println!("Switched to userspace process: PID={}", process_meta.pid);

        Ok(())
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

    let user_programs = userspace_output_path(meta, "");
    assert!(
        user_programs.exists(),
        "Userspace output path does not exist: {}",
        user_programs.display()
    );

    let user_symbols_cache = UserSymbolsCache::load_programs(&user_programs)?;

    let socket = UnixStream::connect(args.qmp_socket.as_deref().unwrap_or("./qmp-socket"))
        .map_err(|e| anyhow::anyhow!("Failed to connect to socket: {}", e))?;

    let mut qmp = qapi::Qmp::from_stream(&socket);

    let info = qmp.handshake()?;
    if args.verbose {
        println!("QMP Info: {info:?}");
    }

    let mut sampler = Sampler::new(qmp, user_symbols_cache, &kernel_path);

    // Handle single sample case (when duration is 0 or very short)
    if args.duration_sec < 1 || args.one_shot {
        let (stack, pid, took) = sampler.sample_with_timing(args)?;
        let symbols = sampler.get_stack_symbols(&stack, args.show_addresses, pid)?;

        println!(
            "Current stack trace (took {took:?}) ({}):",
            pid.map_or("kernel".to_string(), |p| format!(
                "User PID={p} ({})",
                sampler.current_process_name
            ))
        );
        for (i, symbol) in symbols.iter().enumerate() {
            println!("{i:>3}: {symbol}");
        }
        return Ok(());
    }

    // Continuous or timed sampling
    let mut sample_counts: HashMap<String, usize> = HashMap::new();
    let mut processes: HashSet<Option<u64>> = HashSet::new();
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
        match sampler.sample_with_timing(args) {
            Ok((stack, pid, sample_time)) => {
                total_sample_time += sample_time;

                if !stack.is_empty() {
                    let symbol_start = Instant::now();
                    match sampler.get_stack_symbols(&stack, false, pid) {
                        Ok(mut symbols) => {
                            processes.insert(pid);
                            if let Some(pid) = pid {
                                symbols.push(format!(
                                    "User PID={pid} ({})",
                                    sampler.current_process_name
                                ));
                            } else {
                                symbols.push("Kernel".to_string());
                            }
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
                                println!("Failed to resolve symbols: {e}");
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
                    println!("Failed to sample stack: {e}");
                }
            }
        }

        // Sleep for the remaining interval time
        let elapsed = interval_start.elapsed();
        if elapsed < interval_duration {
            std::thread::sleep(interval_duration - elapsed);
        } else if args.verbose {
            println!(
                "Warning: Sampling took longer than interval ({elapsed:?} > {interval_duration:?})"
            );
        }
    }

    let total_elapsed = start_time.elapsed();

    // Print statistics
    println!("\n=== Sampling Statistics ===");
    println!("Total samples: {total_samples}");
    println!("Failed samples: {failed_samples}");
    println!("Unique stacks: {}", sample_counts.len());
    println!("Unique processes (with kernel): {}", processes.len());
    println!("Total time: {total_elapsed:?}");
    if total_samples > 0 {
        println!(
            "Average sample time: {:?}",
            total_sample_time / total_samples as u32
        );
        println!("Total sample time: {total_sample_time:?}");
        println!(
            "Average symbol resolution time: {:?}",
            total_symbol_time / total_samples as u32
        );
        println!("Total symbol resolution time: {total_symbol_time:?}");
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
            writeln!(file, "{stack} {count}")?;
        }

        println!("Folded stack samples written to: {output_path}");
        println!("Generate flamegraph with: flamegraph.pl {output_path} > flamegraph.svg");
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
