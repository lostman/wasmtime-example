(module
  (import "debug" "print" (func $debug_print (param i32) (param i32)))
  (func (export "run") (call $debug_print (i32.const 4) (i32.const 3)))
  (data (i32.const 4) "Hi!")
  (memory (export "memory") 1 1)
)
