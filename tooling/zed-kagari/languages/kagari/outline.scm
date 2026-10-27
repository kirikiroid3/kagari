(function_item
  (visibility_modifier)? @context
  (function_modifiers)? @context
  "fn" @context
  name: (_) @name) @item

(struct_item
  (visibility_modifier)? @context
  "struct" @context
  name: (_) @name) @item

(enum_item
  (visibility_modifier)? @context
  "enum" @context
  name: (_) @name) @item

(enum_variant
  name: (_) @name) @item

(trait_item
  (visibility_modifier)? @context
  "trait" @context
  name: (_) @name) @item

(impl_item
  "impl" @context
  type: (_) @name) @item

(mod_item
  (visibility_modifier)? @context
  "mod" @context
  name: (_) @name) @item

(const_item
  (visibility_modifier)? @context
  "const" @context
  name: (_) @name) @item

(static_item
  (visibility_modifier)? @context
  "static" @context
  name: (_) @name) @item

(type_item
  (visibility_modifier)? @context
  "type" @context
  name: (_) @name) @item
