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
    ($($(#[$attr:tt])? fn $name:ident() $body:block)*) => {
        $($crate::testing::test!(@meta $($attr)? fn $name() $body);)*
    };
    ($(other:tt)*) => {
        compile_error!("Invalid test syntax");
    };
    (@meta should_panic fn $name:ident() $body:block) => {
        $crate::testing::test!(@final true, false, fn $name() $body);
    };
    (@meta ignore fn $name:ident() $body:block) => {
        $crate::testing::test!(@final false, true, fn $name() $body);
    };
    (@meta fn $name:ident() $body:block) => {
        $crate::testing::test!(@final false, false, fn $name() $body);
    };
    (@final $should_panic:expr, $ignore:expr, fn $name:ident() $body:block) => {
        #[cfg(test)]
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
    println!("Running {} tests", tests.len());

    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;

    for test in tests {
        print!("test {} ... ", test.name);
        if test.ignore {
            println!("IGNORED");
            ignored += 1;
            continue;
        }

        let r = unwinding::panic::catch_unwind(|| (test.test_fn)());

        if test.should_panic {
            if r.is_err() {
                passed += 1;
                println!("OK");
            } else {
                failed += 1;
                println!("FAILED (should_panic)");
            }
        } else if r.is_ok() {
            passed += 1;
            println!("OK");
        } else {
            failed += 1;
            println!("FAILED");
        }
    }

    println!("{} passed; {} failed; {} ignored", passed, failed, ignored);

    if failed > 0 {
        qemu::exit(qemu::ExitStatus::Failure);
    } else {
        qemu::exit(qemu::ExitStatus::Success);
    }
}
