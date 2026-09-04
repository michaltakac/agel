;;;; A small, independent Common Lisp reference for Agel's functional kernel.
;;;; It intentionally shares semantic test forms, not evaluator implementation,
;;;; with the Rust seed.

(defpackage :agel-reference
  (:use :cl)
  (:shadow :apply))
(in-package :agel-reference)

(defstruct agel-closure parameters body environment)

(defparameter *globals* (make-hash-table :test #'equal))
(defconstant +i64-min+ (- (expt 2 63)))
(defconstant +i64-max+ (1- (expt 2 63)))

(set-dispatch-macro-character #\# #\t (lambda (stream character argument)
  (declare (ignore stream character argument)) t))
(set-dispatch-macro-character #\# #\f (lambda (stream character argument)
  (declare (ignore stream character argument)) :false))

(defun name (symbol)
  (string-downcase (symbol-name symbol)))

(defun lookup (symbol environment)
  (let ((binding (assoc (name symbol) environment :test #'equal)))
    (if binding
        (cdr binding)
        (multiple-value-bind (value present) (gethash (name symbol) *globals*)
          (if present value (error "unbound Agel name ~A" (name symbol)))))))

(defun bind (parameters arguments environment)
  (unless (= (length parameters) (length arguments))
    (error "Agel closure arity mismatch"))
  (append
   (mapcar (lambda (parameter argument) (cons (name parameter) argument))
           parameters arguments)
   environment))

(defun require-count (name values expected)
  (unless (= (length values) expected)
    (error "Agel ~A expects ~D arguments" name expected))
  values)

(defun eval-sequence (forms environment)
  (let ((result nil))
    (dolist (form forms result)
      (setf result (agel-eval form environment)))))

(defun agel-type (value)
  (cond ((null value) 'nil)
        ((or (eq value t) (eq value :false)) 'bool)
        ((integerp value) 'int)
        ((stringp value) 'string)
        ((symbolp value) 'symbol)
        ((listp value) 'list)
        ((agel-closure-p value) 'callable)
        ((functionp value) 'callable)
        (t (error "unknown Agel value type"))))

(defun truthy (value)
  (not (or (null value) (eq value :false))))

(defun checked (value)
  (unless (<= +i64-min+ value +i64-max+)
    (error "Agel i64 overflow"))
  value)

(defun checked-fold (function identity values)
  (reduce (lambda (left right) (checked (funcall function left right)))
          values :initial-value identity))

(defun agel-add (&rest values)
  (checked-fold #'+ 0 values))

(defun agel-multiply (&rest values)
  (checked-fold #'* 1 values))

(defun agel-subtract (&rest values)
  (when (null values) (error "Agel - requires at least one argument"))
  (if (null (rest values))
      (checked (- (first values)))
      (reduce (lambda (left right) (checked (- left right))) (rest values)
              :initial-value (first values))))

(defun agel-divide (&rest values)
  (when (< (length values) 2) (error "Agel / requires at least two arguments"))
  (reduce (lambda (left right)
            (when (zerop right) (error "Agel division by zero"))
            (checked (truncate left right)))
          (rest values) :initial-value (first values)))

(defun apply (function arguments)
  (cond ((agel-closure-p function)
         (eval-sequence
          (agel-closure-body function)
          (bind (agel-closure-parameters function)
                arguments
                (agel-closure-environment function))))
        ((functionp function) (cl:apply function arguments))
        (t (error "Agel value is not callable"))))

(defun eval-let (bindings body environment)
  (let ((values
          (mapcar (lambda (binding)
                    (cons (name (first binding))
                          (agel-eval (second binding) environment)))
                  bindings)))
    (eval-sequence body (append values environment))))

(defun agel-eval (expression &optional environment)
  (cond
    ((null expression) nil)
    ((or (eq expression t) (eq expression :false)) expression)
    ((or (integerp expression) (stringp expression)) expression)
    ((symbolp expression) (lookup expression environment))
    ((listp expression)
     (let ((head (first expression)) (tail (rest expression)))
       (cond
         ((and (symbolp head) (string= (name head) "quote"))
          (first (require-count "quote" tail 1)))
         ((and (symbolp head) (string= (name head) "if"))
          (require-count "if" tail 3)
          (agel-eval (if (truthy (agel-eval (first tail) environment))
                         (second tail)
                         (third tail))
                     environment))
         ((and (symbolp head) (string= (name head) "fn"))
          (when (< (length tail) 2) (error "Agel fn requires a body"))
          (make-agel-closure :parameters (first tail)
                             :body (rest tail)
                             :environment environment))
         ((and (symbolp head) (string= (name head) "let"))
          (when (< (length tail) 2) (error "Agel let requires a body"))
          (eval-let (first tail) (rest tail) environment))
         ((and (symbolp head) (string= (name head) "begin"))
          (eval-sequence tail environment))
         ((and (symbolp head) (string= (name head) "def"))
          (require-count "def" tail 2)
          (let ((value (agel-eval (second tail) environment)))
            (setf (gethash (name (first tail)) *globals*) value)
            value))
         (t (apply (agel-eval head environment)
                   (mapcar (lambda (item) (agel-eval item environment)) tail))))))
    (t (error "invalid Agel expression"))))

(defun install-builtins ()
  (flet ((put (name function) (setf (gethash name *globals*) function)))
    (put "+" #'agel-add)
    (put "-" #'agel-subtract)
    (put "*" #'agel-multiply)
    (put "/" #'agel-divide)
    (put "=" #'equal)
    (put "list" #'list)
    (put "cons" #'cons)
    (put "car" (lambda (value) (if value (car value) nil)))
    (put "cdr" (lambda (value) (if value (cdr value) nil)))
    (put "count" #'length)
    (put "type-of" #'agel-type)
    (put "apply" #'apply)
    (put "nil" nil)))

(defun print-string (value stream)
  (write-char #\" stream)
  (loop for character across value do
    (case character
      (#\\ (write-string "\\\\" stream))
      (#\" (write-string "\\\"" stream))
      (#\Newline (write-string "\\n" stream))
      (#\Return (write-string "\\r" stream))
      (#\Tab (write-string "\\t" stream))
      (otherwise (write-char character stream))))
  (write-char #\" stream))

(defun print-value (value &optional (stream *standard-output*))
  (cond ((null value) (write-string "nil" stream))
        ((eq value t) (write-string "#t" stream))
        ((eq value :false) (write-string "#f" stream))
        ((integerp value) (princ value stream))
        ((stringp value) (print-string value stream))
        ((symbolp value) (write-string (name value) stream))
        ((agel-closure-p value) (write-string "#<closure>" stream))
        ((functionp value) (write-string "#<builtin>" stream))
        ((listp value)
         (write-char #\( stream)
         (loop for item in value for first = t then nil do
           (unless first (write-char #\Space stream))
           (print-value item stream))
         (write-char #\) stream))
        (t (error "cannot print Agel value"))))

(defun run-conformance (&optional (path "bootstrap/conformance.forms"))
  (clrhash *globals*)
  (install-builtins)
  (with-open-file (input path)
    (loop for form = (read input nil :eof)
          until (eq form :eof) do
            (print-value (agel-eval form))
            (terpri))))

(defun run-error-conformance (&optional (path "bootstrap/conformance-errors.forms"))
  (with-open-file (input path)
    (loop for form = (read input nil :eof)
          until (eq form :eof) do
            (handler-case
                (progn
                  (agel-eval form)
                  (write-line "accepted"))
              (error () (write-line "error"))))))

(run-conformance)
(run-error-conformance)
