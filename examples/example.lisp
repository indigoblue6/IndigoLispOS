; example.lisp - Example Lisp programs for IndigoLispOS

; Basic arithmetic
(+ 1 2 3 4 5)           ; => 15
(- 10 3)                ; => 7
(* 4 5)                 ; => 20
(/ 20 4)                ; => 5

; Variables
(define x 42)
(define y 10)
(+ x y)                 ; => 52

; Conditionals
(if (> x y) "x is greater" "y is greater")  ; => "x is greater"

(if (< 3 5)
    (+ 10 20)
    (+ 30 40))          ; => 30

; Lists
(list 1 2 3 4 5)        ; => (1 2 3 4 5)
(define my-list (list 10 20 30))

(car my-list)           ; => 10
(cdr my-list)           ; => (20 30)
(car (cdr my-list))     ; => 20

; Nested expressions
(+ (* 2 3) (* 4 5))     ; => 26

(if (= (+ 1 1) 2)
    (+ 100 200)
    0)                  ; => 300

; Boolean operations
(= 5 5)                 ; => true
(= 5 6)                 ; => false
(< 3 5)                 ; => true
(> 3 5)                 ; => false

; Complex example: Calculate factorial iteratively
; Note: This would require lambda support (coming in Phase 4)
; (define factorial
;   (lambda (n)
;     (if (< n 2)
;         1
;         (* n (factorial (- n 1))))))

; Define some constants
(define pi 314159)      ; 3.14159 * 100000 (integer approximation)
(define e 271828)       ; 2.71828 * 100000

; Calculate circle area (r=10, scaled by 100000)
(define radius 10)
(/ (* pi (* radius radius)) 100000)  ; => 314 (approximately)

; Sequence operations
(begin
    (define a 1)
    (define b 2)
    (define c 3)
    (+ a b c))          ; => 6

; Update variables
(define counter 0)
counter                 ; => 0
(set! counter 10)
counter                 ; => 10
(set! counter (+ counter 5))
counter                 ; => 15

; String operations (basic)
"Hello, World!"
(define greeting "Hello from IndigoLispOS!")

; Future OS API examples (not yet implemented):
; (os/print "Hello, World!")
; (os/gpio-mode 21 'output)
; (os/gpio-write 21 1)
; (os/delay 1000)
; (os/gpio-write 21 0)
