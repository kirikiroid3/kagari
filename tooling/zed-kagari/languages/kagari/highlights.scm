; Zed uses the last matching pattern, so general patterns come first.

(type_identifier) @type
(primitive_type) @type.builtin
(self_type) @type.builtin
(self) @variable.special

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z0-9_]+$"))

(field_identifier) @property
(shorthand_field_identifier) @property

(function_item name: (identifier) @function)
(call_expression function: (identifier) @function)
(call_expression function: (scoped_identifier name: (identifier) @function))
(call_expression function: (field_expression field: (field_identifier) @function))
(generic_function function: (identifier) @function)
(generic_function function: (scoped_identifier name: (identifier) @function))
(generic_function function: (field_expression field: (field_identifier) @function))
(form_expression name: (identifier) @function @function.special)
(form_attribute name: (identifier) @property)

(enum_variant name: (identifier) @variant)
(mod_item name: (identifier) @module)

(attribute_item) @attribute
(label) @label

(line_comment) @comment
(block_comment) @comment
(doc_comment) @comment.doc

(string_literal) @string
(escape_sequence) @string.escape
(integer_literal) @number
(decimal_literal) @number
(boolean_literal) @boolean

[
  "as"
  "break"
  "const"
  "continue"
  "crate"
  "else"
  "enum"
  "fn"
  "for"
  "if"
  "impl"
  "in"
  "let"
  "loop"
  "match"
  "mod"
  "mut"
  "pub"
  "return"
  "static"
  "struct"
  "super"
  "trait"
  "type"
  "use"
  "while"
  "pushdown"
  "scene"
] @keyword

[
  "!"
  "!="
  "%"
  "%="
  "&"
  "&&"
  "&="
  "*"
  "*="
  "+"
  "+="
  "-"
  "-="
  "->"
  ".."
  "..="
  "/"
  "/="
  "<<"
  "<<="
  "<="
  "="
  "=="
  "=>"
  ">="
  ">>"
  ">>="
  "?"
  "^"
  "^="
  "|"
  "|="
  "||"
] @operator

["(" ")" "[" "]" "{" "}" "<" ">"] @punctuation.bracket
["," ";" ":" "::" "."] @punctuation.delimiter

(form_expression "!" @punctuation.special)
