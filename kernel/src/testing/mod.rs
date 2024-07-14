#[cfg(test)]
use crate::hw::qemu;

#[cfg(test)]
pub struct TestCase {
    pub name: &'static str,
    pub ignore: bool,
    pub source: &'static str,
    pub line: u32,
    pub should_panic: bool,
    pub test_fn: &'static dyn Fn(),
}

#[macro_export]
macro_rules! test {
    // The entry point
    ($($(#[$($attr:tt)+])* fn $name:ident() $body:block)*) => {
        $($crate::testing::test!(@meta_chain $([$($attr)+])* => [] {false, false} fn $name() $body);)*
    };
    // any other entrypoints are errors
    ($(other:tt)*) => {
        compile_error!("Invalid test syntax");
    };
    // The final chain, if we don't have any more `meta` attributes, we build the thing
    (@meta_chain
        => [$($builtmeta:tt)*]
        {$should_panic:expr, $ignore:expr}
        fn $name:ident() $body:block
    ) => {
        $crate::testing::test!(@final [$($builtmeta)*] {$should_panic, $ignore}  fn $name() $body);
    };
    // If we have meta `should_panic` or `ignore`, we modify the variable we are using
    (@meta_chain
        [should_panic] $([$($rest:tt)+])* => [$($builtmeta:tt)*]
        {$should_panic:expr, $ignore:expr}
        fn $name:ident() $body:block
    ) => {
        $crate::testing::test!(@meta_chain $([$($rest)+])* =>
        [
            $($builtmeta)*
        ]
        {true, $ignore} fn $name() $body);
    };
    (@meta_chain
        [ignore] $([$($rest:tt)+])* => [$($builtmeta:tt)*]
        {$should_panic:expr, $ignore:expr}
        fn $name:ident() $body:block
    ) => {
        $crate::testing::test!(@meta_chain $([$($rest)+])* =>
        [
            $($builtmeta)*
        ]
        {$should_panic, true} fn $name() $body);
    };
    // Any other attributes are passed as is
    (@meta_chain
        [$($first:tt)+] $([$($rest:tt)+])* => [$($builtmeta:tt)*]
        {$should_panic:expr, $ignore:expr}
        fn $name:ident() $body:block
    ) => {
        $crate::testing::test!(@meta_chain $([$($rest)+])* =>
        [
            #[$($first)+]
            $($builtmeta)*
        ]
        {$should_panic, $ignore} fn $name() $body);
    };
    // final construction
    (@final
        [$($builtmeta:tt)*]
        {$should_panic:expr, $ignore:expr}
        fn $name:ident() $body:block
    ) => {
        #[cfg(test)]
        $($builtmeta)*
        fn $name() $body
        #[cfg(test)]
        #[test_case]
        const $name: $crate::testing::TestCase = $crate::testing::TestCase {
            name: concat!(module_path!(), "::", stringify!($name)),
            ignore: $ignore,
            source: file!(),
            line: line!(),
            should_panic: $should_panic,
            test_fn: &$name,
        };
    };
}

pub use test;

#[cfg(test)]
pub fn test_runner(tests: &[&TestCase]) {
    use alloc::{string::String, vec::Vec};

    use crate::{io::console, panic_handler};

    println!("Running {} tests", tests.len());

    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;

    let mut failed_buffers = Vec::new();

    for test in tests {
        print!("test {} ... ", test.name);
        if test.ignore {
            println!("IGNORED");
            ignored += 1;
            continue;
        }

        assert!(console::start_capture().is_none());

        let r = panic_handler::catch_unwind(|| (test.test_fn)());

        let buffer = console::stop_capture().unwrap();

        if r.is_ok() {
            if test.should_panic {
                failed += 1;
                println!("FAILED (should_panic)");
            } else {
                passed += 1;
                println!("OK");
            }
        } else if test.should_panic {
            passed += 1;
            println!("OK");
        } else {
            failed += 1;
            println!("FAILED");

            failed_buffers.push((test.name, buffer));
        }
    }

    if !failed_buffers.is_empty() {
        println!("\n\nfailures:\n");

        for (name, panic) in failed_buffers {
            println!("--- {name} ---\n{panic}\n");
        }

        println!();
    }

    println!("{} passed; {} failed; {} ignored", passed, failed, ignored);

    if failed > 0 {
        qemu::exit(qemu::ExitStatus::Failure);
    } else {
        qemu::exit(qemu::ExitStatus::Success);
    }
}
