(function() {var type_impls = {
"kernel":[["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-InterruptHandler-for-extern+%22cdecl%22+fn(%26mut+InterruptAllSavedState)\" class=\"impl\"><a class=\"src rightside\" href=\"src/kernel/cpu/interrupts/mod.rs.html#99-106\">source</a><a href=\"#impl-InterruptHandler-for-extern+%22cdecl%22+fn(%26mut+InterruptAllSavedState)\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"kernel/cpu/interrupts/trait.InterruptHandler.html\" title=\"trait kernel::cpu::interrupts::InterruptHandler\">InterruptHandler</a> for extern &quot;cdecl&quot; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/core/primitive.fn.html\">fn</a>(state: &amp;mut <a class=\"struct\" href=\"kernel/cpu/idt/struct.InterruptAllSavedState.html\" title=\"struct kernel::cpu::idt::InterruptAllSavedState\">InterruptAllSavedState</a>)</h3></section></summary><div class=\"impl-items\"><section id=\"method.allocate_and_set_handler\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/kernel/cpu/interrupts/mod.rs.html#100-105\">source</a><a href=\"#method.allocate_and_set_handler\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"kernel/cpu/interrupts/trait.InterruptHandler.html#tymethod.allocate_and_set_handler\" class=\"fn\">allocate_and_set_handler</a>(handler: Self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/core/primitive.u8.html\">u8</a></h4></section></div></details>","InterruptHandler","kernel::cpu::idt::InterruptHandlerWithAllState"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-InterruptHandler-for-extern+%22x86-interrupt%22+fn(InterruptStackFrame64)\" class=\"impl\"><a class=\"src rightside\" href=\"src/kernel/cpu/interrupts/mod.rs.html#90-97\">source</a><a href=\"#impl-InterruptHandler-for-extern+%22x86-interrupt%22+fn(InterruptStackFrame64)\" class=\"anchor\">§</a><h3 class=\"code-header\">impl <a class=\"trait\" href=\"kernel/cpu/interrupts/trait.InterruptHandler.html\" title=\"trait kernel::cpu::interrupts::InterruptHandler\">InterruptHandler</a> for extern &quot;x86-interrupt&quot; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/core/primitive.fn.html\">fn</a>(frame: <a class=\"struct\" href=\"kernel/cpu/idt/struct.InterruptStackFrame64.html\" title=\"struct kernel::cpu::idt::InterruptStackFrame64\">InterruptStackFrame64</a>)</h3></section></summary><div class=\"impl-items\"><section id=\"method.allocate_and_set_handler\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/kernel/cpu/interrupts/mod.rs.html#91-96\">source</a><a href=\"#method.allocate_and_set_handler\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"kernel/cpu/interrupts/trait.InterruptHandler.html#tymethod.allocate_and_set_handler\" class=\"fn\">allocate_and_set_handler</a>(handler: Self) -&gt; <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/core/primitive.u8.html\">u8</a></h4></section></div></details>","InterruptHandler","kernel::cpu::idt::BasicInterruptHandler"]]
};if (window.register_type_impls) {window.register_type_impls(type_impls);} else {window.pending_type_impls = type_impls;}})()