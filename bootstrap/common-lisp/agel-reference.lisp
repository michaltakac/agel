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

;;; Persistent maps are insertion-ordered entry lists, as in the Rust seed:
;;; replacing a key keeps its position, and equality is structural and ordered.
(defstruct agel-map entries)

(defun agel-equal (left right)
  (cond ((and (agel-map-p left) (agel-map-p right))
         (agel-equal (agel-map-entries left) (agel-map-entries right)))
        ((or (agel-map-p left) (agel-map-p right)) nil)
        ((and (consp left) (consp right))
         (and (agel-equal (car left) (car right))
              (agel-equal (cdr left) (cdr right))))
        (t (equal left right))))

(defun agel-type (value)
  (cond ((null value) 'nil)
        ((or (eq value t) (eq value :false)) 'bool)
        ((integerp value) 'int)
        ((stringp value) 'string)
        ((symbolp value) 'symbol)
        ((listp value) 'list)
        ((agel-map-p value) 'map)
        ((agel-closure-p value) 'callable)
        ((functionp value) 'callable)
        (t (error "unknown Agel value type"))))

(defun require-map (name value)
  (unless (agel-map-p value) (error "Agel ~A expects a map" name))
  value)

(defun map-insert (entries key value)
  (let ((existing (assoc key entries :test #'agel-equal)))
    (if existing
        (mapcar (lambda (entry)
                  (if (eq entry existing) (cons key value) entry))
                entries)
        (append entries (list (cons key value))))))

(defun agel-dict (&rest arguments)
  (unless (evenp (length arguments)) (error "Agel dict expects key/value pairs"))
  (let ((entries nil))
    (loop for (key value) on arguments by #'cddr do
      (setf entries (map-insert entries key value)))
    (make-agel-map :entries entries)))

(defun agel-get (map key)
  (cdr (assoc key (agel-map-entries (require-map "get" map)) :test #'agel-equal)))

(defun agel-has-key (map key)
  (if (assoc key (agel-map-entries (require-map "has-key?" map)) :test #'agel-equal)
      t
      :false))

(defun agel-assoc (map key value)
  (make-agel-map
   :entries (map-insert (agel-map-entries (require-map "assoc" map)) key value)))

(defun agel-dissoc (map key)
  (make-agel-map
   :entries (remove-if (lambda (entry) (agel-equal (car entry) key))
                       (agel-map-entries (require-map "dissoc" map)))))

(defun agel-keys (map)
  (mapcar #'car (agel-map-entries (require-map "keys" map))))

(defun agel-count (value)
  (cond ((null value) 0)
        ((listp value) (length value))
        ((agel-map-p value) (length (agel-map-entries value)))
        ((stringp value) (length value))
        (t (error "Agel count cannot inspect this value"))))

;;; Text mechanisms are byte-oriented over UTF-8, exactly like the seed.
(defun utf8 (value)
  (unless (stringp value) (error "Agel text operation expects text"))
  (sb-ext:string-to-octets value :external-format :utf-8))

(defun byte-offset (value)
  (unless (and (integerp value) (>= value 0)) (error "Agel text offset must be non-negative"))
  value)

(defun utf8-boundary-p (octets index)
  (or (= index (length octets))
      (/= (logand (aref octets index) #xC0) #x80)))

(defun agel-text-bytes (text)
  (length (utf8 text)))

(defun agel-text-byte (text offset)
  (let ((octets (utf8 text)) (index (byte-offset offset)))
    (unless (< index (length octets)) (error "Agel text byte out of range"))
    (aref octets index)))

(defun agel-text-slice (text start end)
  (let ((octets (utf8 text)) (from (byte-offset start)) (to (byte-offset end)))
    (unless (and (<= from to) (<= to (length octets))
                 (utf8-boundary-p octets from) (utf8-boundary-p octets to))
      (error "Agel text slice is not a character boundary"))
    (sb-ext:octets-to-string (subseq octets from to) :external-format :utf-8)))

(defun agel-text-concat (left right)
  (unless (and (stringp left) (stringp right)) (error "Agel text-concat expects text"))
  (concatenate 'string left right))

(defun agel-text-symbol (text)
  (unless (stringp text) (error "Agel text-symbol expects text"))
  (intern (string-upcase text) :agel-reference))

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
  (unless (listp bindings) (error "Agel let requires bindings"))
  (let ((values
          (mapcar (lambda (binding)
                    (unless (and (listp binding) (= (length binding) 2)
                                 (agel-name-p (first binding)))
                      (error "Agel let requires name/value pairs"))
                    (cons (name (first binding))
                          (agel-eval (second binding) environment)))
                  bindings)))
    ;; All initializers use the outer scope; the last repeated name wins.
    (eval-sequence body (append (reverse values) environment))))

(defun agel-name-p (value)
  (and (symbolp value) value (not (member value '(t :false)))))

(defun validate-parameters (parameters)
  (unless (and (listp parameters) (every #'agel-name-p parameters))
    (error "Agel fn requires symbolic parameters"))
  (unless (= (length parameters) (length (remove-duplicates parameters)))
    (error "Agel fn repeats a parameter"))
  parameters)

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
          (make-agel-closure :parameters (validate-parameters (first tail))
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
    (put "=" (lambda (left right) (if (agel-equal left right) t :false)))
    (put "list" #'list)
    (put "cons" (lambda (head tail)
                  (unless (listp tail) (error "Agel cons expects a list"))
                  (cons head tail)))
    (put "car" (lambda (value)
                 (unless (listp value) (error "Agel car expects a list"))
                 (if value (car value) nil)))
    (put "cdr" (lambda (value)
                 (unless (listp value) (error "Agel cdr expects a list"))
                 (if value (cdr value) nil)))
    (put "count" #'agel-count)
    (put "type-of" #'agel-type)
    (put "apply" #'apply)
    (put "dict" #'agel-dict)
    (put "get" #'agel-get)
    (put "has-key?" #'agel-has-key)
    (put "assoc" #'agel-assoc)
    (put "dissoc" #'agel-dissoc)
    (put "keys" #'agel-keys)
    (put "text-bytes" #'agel-text-bytes)
    (put "text-byte" #'agel-text-byte)
    (put "text-slice" #'agel-text-slice)
    (put "text-concat" #'agel-text-concat)
    (put "text-symbol" #'agel-text-symbol)
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
        ((agel-map-p value)
         (write-char #\{ stream)
         (loop for (key . entry) in (agel-map-entries value) for first = t then nil do
           (unless first (write-char #\Space stream))
           (print-value key stream)
           (write-char #\Space stream)
           (print-value entry stream))
         (write-char #\} stream))
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
(run-conformance "bootstrap/metacircular.forms")
(run-error-conformance "bootstrap/metacircular-errors.forms")
