use crate::dynasmrt::{
    x64::Assembler, AssemblyOffset, DynamicLabel, DynasmApi, DynasmLabelApi, ExecutableBuffer,
};

pub struct CGContext {
    pub putchar: unsafe extern "C" fn (x: u8),
    pub getchar: unsafe extern "C" fn () -> u8,
    pub opt_level: u8,
}

pub struct Codegen {
    ctx: CGContext
}

pub struct JitOutput {
    pub buffer: Vec<u8>,
    pub loop_end_patch_offsets: Vec<usize>,
}

#[derive(Copy, Clone, Debug)]
enum Token {
    Forward(u32),
    Backward(u32),
    Add(u8),
    Sub(u8),
    Output,
    Input,
    LoopBegin,
    LoopEnd,
}

impl Codegen {
    pub fn new(ctx: CGContext) -> Codegen {
        Codegen {
            ctx: ctx,
        }
    }

    pub fn translate(&self, input: &str) -> JitOutput {
        let mut tokens: Vec<Token> = vec![];
        let mut chars = input.chars().peekable();
        loop {
            let c = if let Some(c) = chars.next() {
                c
            } else {
                break
            };
            match c {
                '>' => {
                    let mut n: u32 = 1;
                    if self.ctx.opt_level > 0 {
                        while chars.peek() == Some(&'>') {
                            n += 1;
                            chars.next().unwrap();
                        }
                    }
                    tokens.push(Token::Forward(n));
                },
                '<' => {
                    let mut n: u32 = 1;
                    if self.ctx.opt_level > 0 {
                        while chars.peek() == Some(&'<') {
                            n += 1;
                            chars.next().unwrap();
                        }
                    }
                    tokens.push(Token::Backward(n));
                }
                '+' => {
                    let mut n: u8 = 1;
                    if self.ctx.opt_level > 0 {
                        while chars.peek() == Some(&'+') {
                            n += 1;
                            chars.next().unwrap();
                        }
                    }
                    tokens.push(Token::Add(n));
                }
                '-' => {
                    let mut n: u8 = 1;
                    if self.ctx.opt_level > 0 {
                        while chars.peek() == Some(&'-') {
                            n += 1;
                            chars.next().unwrap();
                        }
                    }
                    tokens.push(Token::Sub(n));
                }
                '.' => tokens.push(Token::Output),
                ',' => tokens.push(Token::Input),
                '[' => tokens.push(Token::LoopBegin),
                ']' => tokens.push(Token::LoopEnd),
                _ => {}
            };
        }
        self.do_translate(&tokens)
    }

    fn do_translate(&self, input: &[Token]) -> JitOutput {
        // rdi: memory
        let mut out: Assembler = Assembler::new().unwrap();
        let mut labels: Vec<(DynamicLabel, DynamicLabel)> = vec![];
        let mut loop_end_patch_offsets: Vec<usize> = vec![];

        for t in input {
            match *t {
                Token::Forward(n) => dynasm!(
                    out
                    ; add rdi, n as i32
                ),
                Token::Backward(n) => dynasm!(
                    out
                    ; sub rdi, n as i32
                ),
                Token::Add(n) => dynasm!(
                    out
                    ; add BYTE [rdi], n as i8
                ),
                Token::Sub(n) => dynasm!(
                    out
                    ; sub BYTE [rdi], n as i8
                ),
                Token::Output => dynasm!(
                    out
                    ; push rdi
                    ; movzx edi, BYTE [rdi]
                    ; mov rax, QWORD self.ctx.putchar as usize as i64
                    ; call rax
                    ; pop rdi
                ),
                Token::Input => dynasm!(
                    out
                    ; push rdi
                    ; mov rax, QWORD self.ctx.getchar as usize as i64
                    ; call rax
                    ; pop rdi
                    ; mov BYTE [rdi], al
                ),
                Token::LoopBegin => {
                    let (start, end) = (out.new_dynamic_label(), out.new_dynamic_label());
                    labels.push((start, end));
                    dynasm!(
                        out
                        ; cmp BYTE [rdi], 0
                        ; je =>end
                        ; =>start
                    );
                },
                Token::LoopEnd => {
                    loop_end_patch_offsets.push(out.offset().0);
                    let (start, end) = labels.pop().unwrap();
                    dynasm!(
                        out
                        ; nop
                        ; cmp BYTE [rdi], 0
                        ; jne =>start
                        ; =>end
                    );
                },
                _ => {}
            }
        }
        dynasm!(out ; ret);

        JitOutput {
            buffer: out.finalize().unwrap().to_vec(),
            loop_end_patch_offsets: loop_end_patch_offsets,
        }
    }
}
