

.macro interrupt_vector_label number
    .quad interrupt_vector_\number
.endm

.section .rodata
.global interrupt_vector_table
.align 16
interrupt_vector_table:
.altmacro
.set i, 0
.rept 256
    interrupt_vector_label %i
    .set i, i+1
.endr


# define the interrupt vector macro
.macro interrupt_vector number
    .global interrupt_vector_\number
    .align 16
    interrupt_vector_\number:
        push 0  # error code
        push \number
        jmp interrupt_vector_global
.endm

# define the interrupt vector macro with errors
.macro interrupt_vector_error number
    .global interrupt_vector_error_\number
    .align 16
    interrupt_vector_\number:

        push \number
        jmp interrupt_vector_global
.endm

# create the interrupt vectors
.section .text
interrupt_vector 0  # divide by zero
interrupt_vector 1  # debug
interrupt_vector 2  # non-maskable interrupt
interrupt_vector 3  # breakpoint
interrupt_vector 4  # overflow
interrupt_vector 5  # bound range exceeded
interrupt_vector 6  # invalid opcode
interrupt_vector 7  # device not available
interrupt_vector_error 8  # double fault
interrupt_vector 9  # coprocessor segment overrun
interrupt_vector_error 10  # invalid TSS
interrupt_vector_error 11  # segment not present
interrupt_vector_error 12  # stack-segment fault
interrupt_vector_error 13  # general protection fault
interrupt_vector_error 14  # page fault
interrupt_vector 15  # reserved
interrupt_vector 16  # x87 floating-point exception
interrupt_vector_error 17  # alignment check
interrupt_vector 18  # machine check
interrupt_vector 19  # SIMD floating-point exception
interrupt_vector 20  # reserved
interrupt_vector 21  # control protection exception
interrupt_vector 22  # reserved
interrupt_vector 23  # reserved
interrupt_vector 24  # reserved
interrupt_vector 25  # reserved
interrupt_vector 26  # reserved
interrupt_vector 27  # reserved
interrupt_vector 28  # hypervisor injection exception
interrupt_vector 29  # vmm communication exception
interrupt_vector 30  # security exception
interrupt_vector 31  # reserved

# user exceptions
.altmacro
.set i, 32
.rept 256-32
    interrupt_vector %i
    .set i, i+1
.endr

.global interrupt_vector_global
interrupt_vector_global:
    # save the registers to the stack
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rbp
    push rdi
    push rsi
    push rdx
    push rcx
    push rbx
    push rax
    mov rax, dr7
    push rax
    mov rax, dr6
    push rax
    mov rax, dr5
    push rax
    mov rax, dr4
    push rax
    mov rax, dr3
    push rax
    mov rax, dr2
    push rax
    mov rax, dr1
    push rax
    mov rax, dr0
    push rax
    mov rax, gs
    push rax
    mov rax, fs
    push rax
    mov rax, es
    push rax
    mov rax, ds
    push rax

    # call rust
    call rust_interrupt_handler_for_all_state

    # restore the registers from the stack
    pop rax
    mov ds, rax
    pop rax
    mov es, rax
    pop rax
    mov fs, rax
    pop rax
    mov gs, rax
    pop rax
    mov dr0, rax
    pop rax
    mov dr1, rax
    pop rax
    mov dr2, rax
    pop rax
    mov dr3, rax
    pop rax
    mov dr4, rax
    pop rax
    mov dr5, rax
    pop rax
    mov dr6, rax
    pop rax
    mov dr7, rax
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rsi
    pop rdi
    pop rbp
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15

    # remove the error number and the code
    # when there is an error code, i.e. we are in an exception handler,
    # we already have error code, but in those cases, we cannot recover anyway,
    # so probably won't be back here
    add rsp, 16

    # return from interrupt
    iretq
