use llvm_sys::LLVMOpcode;
use llvm_sys::core::*;
use llvm_sys::prelude::*;

mod iter;
use self::iter::{BlocksIterableFunction, FunctionsIterableModule, GlobalsIterableModule,
                 InstructionsIterableBlock};

pub trait GlobalValueVisitor {
    fn visit_global_value(&mut self, value: LLVMValueRef) -> bool;
}

pub trait FunctionVisitor {
    fn visit_function(&mut self, function: LLVMValueRef) -> bool;
}

pub trait CallVisitor {
    fn visit_call(&mut self, instruction: LLVMValueRef) -> bool;
}

pub struct PassRunner {
    module: LLVMModuleRef,
}

impl PassRunner {
    pub fn new(module: LLVMModuleRef) -> Self {
        PassRunner { module }
    }

    pub fn run_globals_visitor<V: GlobalValueVisitor>(&self, visitor: &mut V) {
        let mut touched = true;

        while touched {
            touched = false;

            for global in self.module.globals_iter() {
                touched |= visitor.visit_global_value(global);
            }
        }
    }

    pub fn run_calls_visitor<V: CallVisitor>(&self, visitor: &mut V) {
        let mut touched = true;

        while touched {
            touched = false;

            for function in self.module.functions_iter() {
                for block in function.blocks_iter() {
                    for instruction in block.instructions_iter() {
                        let code = unsafe { LLVMGetInstructionOpcode(instruction) };

                        if code == LLVMOpcode::LLVMCall {
                            touched |= visitor.visit_call(instruction);
                        }
                    }
                }
            }
        }
    }
}
