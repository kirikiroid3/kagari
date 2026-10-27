/**
 * @file Kagari (.kis/.kix) grammar for tree-sitter
 * @license Apache-2.0
 *
 * Derived from tree-sitter-rust's grammar.js
 * (https://github.com/tree-sitter/tree-sitter-rust, MIT License,
 * Copyright (c) 2017 Maxim Sokolov), cut down to Kagari.
 *
 * The authority for what is accepted is the hand-written parser in
 * `core/kagari-syntax/src/cst/parser/*.rs` and the lexer in `core/kagari-syntax/src/lex/mod.rs`.
 * This grammar only drives highlighting, brackets, indentation and outlines, so it is more
 * permissive than that parser in a few places:
 *
 * - Forms (`name!(…)`, `name![…]`, `name!{…}`) hold attributes, arms and expressions separated
 *   by optional commas. `t![…]` text has no rule of its own: its strings, rubies, effects and
 *   interpolations parse as juxtaposed expressions.
 * - Block comments do not nest.
 * - Invalid string escapes are accepted; the compiler reports them.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

// Binding powers, in the order of `core/kagari-syntax/src/cst/parser/exprs.rs`.
const PREC = {
  call: 15,
  field: 14,
  try: 13,
  unary: 12,
  cast: 11,
  multiplicative: 10,
  additive: 9,
  shift: 8,
  bitand: 7,
  bitxor: 6,
  bitor: 5,
  comparative: 4,
  and: 3,
  or: 2,
  range: 1,
  assign: 0,
  closure: -1,
};

const integerTypes = [
  'u8', 'u16', 'u32', 'u64', 'u128',
  'i8', 'i16', 'i32', 'i64', 'i128',
  'usize', 'isize',
];

const primitiveTypes = integerTypes.concat(['bool']);

// Tokens allowed inside an attribute's token tree, besides nested trees.
const TOKEN_TREE_PUNCTUATION = [
  '+', '-', '*', '/', '%', '^', '!', '&', '|', '&&', '||', '<<', '>>', '+=', '-=', '*=', '/=',
  '%=', '^=', '&=', '|=', '<<=', '>>=', '=', '==', '!=', '>', '<', '>=', '<=', '_', '.', '..',
  '..=', ',', ';', ':', '::', '->', '=>', '#', '?',
];

const KEYWORDS = [
  'as', 'break', 'const', 'continue', 'crate', 'else', 'enum', 'fn', 'for', 'if', 'impl', 'in',
  'let', 'loop', 'match', 'mod', 'mut', 'pub', 'return', 'static', 'struct', 'super', 'trait',
  'type', 'use', 'while',
];

module.exports = grammar({
  name: 'kagari',

  extras: $ => [
    /\s/,
    $.line_comment,
    $.doc_comment,
    $.block_comment,
  ],

  supertypes: $ => [
    $._expression,
    $._type,
    $._literal,
    $._literal_pattern,
    $._pattern,
  ],

  inline: $ => [
    $._path,
    $._type_identifier,
    $._field_identifier,
    $._tokens,
  ],

  conflicts: _ => [],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._item),

    // ---- items ------------------------------------------------------------------------------

    _item: $ => choice(
      $.attribute_item,
      $.function_item,
      $.const_item,
      $.static_item,
      $.struct_item,
      $.enum_item,
      $.type_item,
      $.trait_item,
      $.impl_item,
      $.mod_item,
      $.use_declaration,
    ),

    attribute_item: $ => seq(
      '#',
      '[',
      $.attribute,
      ']',
    ),

    attribute: $ => seq(
      $._path,
      optional(field('arguments', $.token_tree)),
    ),

    token_tree: $ => choice(
      seq('(', repeat($._tokens), ')'),
      seq('[', repeat($._tokens), ']'),
      seq('{', repeat($._tokens), '}'),
    ),

    _tokens: $ => choice(
      $.token_tree,
      $._literal,
      $.identifier,
      $.label,
      $.self,
      $.self_type,
      alias(choice(...primitiveTypes), $.primitive_type),
      prec.right(repeat1(choice(...TOKEN_TREE_PUNCTUATION))),
      ...KEYWORDS,
      'pushdown',
      'scene',
    ),

    visibility_modifier: _ => 'pub',

    function_item: $ => seq(
      optional($.visibility_modifier),
      optional($.function_modifiers),
      'fn',
      field('name', $.identifier),
      field('type_parameters', optional($.type_parameters)),
      field('parameters', $.parameters),
      optional(seq('->', field('return_type', $._type))),
      choice(field('body', $.block), ';'),
    ),

    function_modifiers: _ => choice('const', 'pushdown'),

    parameters: $ => seq(
      '(',
      sepBy(',', choice($.parameter, $.self_parameter)),
      optional(','),
      ')',
    ),

    self_parameter: $ => seq(
      optional('&'),
      optional('mut'),
      $.self,
    ),

    parameter: $ => seq(
      field('pattern', $._pattern),
      ':',
      field('type', $._type),
    ),

    struct_item: $ => seq(
      optional($.visibility_modifier),
      'struct',
      field('name', $._type_identifier),
      field('type_parameters', optional($.type_parameters)),
      choice(
        field('body', $.field_declaration_list),
        seq(field('body', $.ordered_field_declaration_list), ';'),
        ';',
      ),
    ),

    enum_item: $ => seq(
      optional($.visibility_modifier),
      'enum',
      field('name', $._type_identifier),
      field('type_parameters', optional($.type_parameters)),
      field('body', $.enum_variant_list),
    ),

    enum_variant_list: $ => seq(
      '{',
      sepBy(',', seq(repeat($.attribute_item), $.enum_variant)),
      optional(','),
      '}',
    ),

    enum_variant: $ => seq(
      field('name', $.identifier),
      field('body', optional(choice(
        $.field_declaration_list,
        $.ordered_field_declaration_list,
      ))),
    ),

    field_declaration_list: $ => seq(
      '{',
      sepBy(',', seq(repeat($.attribute_item), $.field_declaration)),
      optional(','),
      '}',
    ),

    field_declaration: $ => seq(
      optional($.visibility_modifier),
      field('name', $._field_identifier),
      ':',
      field('type', $._type),
    ),

    ordered_field_declaration_list: $ => seq(
      '(',
      sepBy(',', seq(
        repeat($.attribute_item),
        optional($.visibility_modifier),
        field('type', $._type),
      )),
      optional(','),
      ')',
    ),

    type_item: $ => seq(
      optional($.visibility_modifier),
      'type',
      field('name', $._type_identifier),
      field('type_parameters', optional($.type_parameters)),
      field('bounds', optional($.trait_bounds)),
      optional(seq('=', field('type', $._type))),
      ';',
    ),

    const_item: $ => seq(
      optional($.visibility_modifier),
      'const',
      field('name', $.identifier),
      ':',
      field('type', $._type),
      optional(seq('=', field('value', $._expression))),
      ';',
    ),

    static_item: $ => seq(
      optional($.visibility_modifier),
      'static',
      optional('mut'),
      field('name', $.identifier),
      ':',
      field('type', $._type),
      '=',
      field('value', $._expression),
      ';',
    ),

    mod_item: $ => seq(
      optional($.visibility_modifier),
      'mod',
      field('name', $.identifier),
      choice(';', field('body', $.declaration_list)),
    ),

    declaration_list: $ => seq(
      '{',
      repeat($._item),
      '}',
    ),

    impl_item: $ => seq(
      optional($.visibility_modifier),
      'impl',
      field('type_parameters', optional($.type_parameters)),
      optional(seq(
        field('trait', choice(
          $._type_identifier,
          $.scoped_type_identifier,
          $.generic_type,
        )),
        'for',
      )),
      field('type', $._type),
      field('body', $.declaration_list),
    ),

    trait_item: $ => seq(
      optional($.visibility_modifier),
      'trait',
      field('name', $._type_identifier),
      field('type_parameters', optional($.type_parameters)),
      field('bounds', optional($.trait_bounds)),
      field('body', $.declaration_list),
    ),

    trait_bounds: $ => seq(
      ':',
      sepBy1('+', $._type),
    ),

    type_parameters: $ => prec(1, seq(
      '<',
      sepBy1(',', choice($.type_parameter, $.const_parameter)),
      optional(','),
      '>',
    )),

    type_parameter: $ => seq(
      field('name', $._type_identifier),
      optional(field('bounds', $.trait_bounds)),
    ),

    const_parameter: $ => seq(
      'const',
      field('name', $.identifier),
      ':',
      field('type', $._type),
    ),

    use_declaration: $ => seq(
      optional($.visibility_modifier),
      'use',
      field('argument', $._use_clause),
      ';',
    ),

    _use_clause: $ => choice(
      $._path,
      $.use_as_clause,
      $.use_list,
      $.scoped_use_list,
      $.use_wildcard,
    ),

    scoped_use_list: $ => seq(
      field('path', $._path),
      '::',
      field('list', $.use_list),
    ),

    use_list: $ => seq(
      '{',
      sepBy(',', $._use_clause),
      optional(','),
      '}',
    ),

    use_as_clause: $ => seq(
      field('path', $._path),
      'as',
      field('alias', choice($.identifier, '_')),
    ),

    use_wildcard: $ => seq(
      optional(seq($._path, '::')),
      '*',
    ),

    // ---- types ------------------------------------------------------------------------------

    _type: $ => choice(
      $.reference_type,
      $.generic_type,
      $.scoped_type_identifier,
      $.tuple_type,
      $.unit_type,
      $.array_type,
      $.function_type,
      $.closure_type,
      $.self_type,
      $._type_identifier,
      alias('_', $.infer_type),
      alias(choice(...primitiveTypes), $.primitive_type),
    ),

    array_type: $ => seq(
      '[',
      field('element', $._type),
      ';',
      field('length', $._expression),
      ']',
    ),

    function_type: $ => seq(
      optional(alias($._function_type_modifiers, $.function_modifiers)),
      'fn',
      field('parameters', alias($.type_parameter_list, $.parameters)),
      optional(seq('->', field('return_type', $._type))),
    ),

    _function_type_modifiers: _ => choice('const', 'pushdown', 'scene'),

    closure_type: $ => seq(
      optional(alias($._closure_type_modifiers, $.function_modifiers)),
      field('trait', alias('Fn', $.type_identifier)),
      field('parameters', alias($.type_parameter_list, $.parameters)),
      optional(seq('->', field('return_type', $._type))),
    ),

    _closure_type_modifiers: _ => choice('const', 'pushdown'),

    type_parameter_list: $ => seq(
      '(',
      sepBy(',', $._type),
      optional(','),
      ')',
    ),

    tuple_type: $ => seq(
      '(',
      sepBy1(',', $._type),
      optional(','),
      ')',
    ),

    unit_type: _ => seq('(', ')'),

    generic_type: $ => prec(1, seq(
      field('type', choice(
        $._type_identifier,
        $.scoped_type_identifier,
      )),
      field('type_arguments', $.type_arguments),
    )),

    generic_type_with_turbofish: $ => seq(
      field('type', choice(
        $._type_identifier,
        $.scoped_identifier,
      )),
      '::',
      field('type_arguments', $.type_arguments),
    ),

    type_arguments: $ => seq(
      token(prec(1, '<')),
      sepBy1(',', choice(
        $._type,
        $._literal,
        $.negative_literal,
        $.block,
      )),
      optional(','),
      '>',
    ),

    reference_type: $ => seq(
      '&',
      optional('mut'),
      field('type', $._type),
    ),

    // ---- expressions ------------------------------------------------------------------------

    _expression_except_range: $ => choice(
      $.unary_expression,
      $.reference_expression,
      $.try_expression,
      $.binary_expression,
      $.assignment_expression,
      $.compound_assignment_expr,
      $.type_cast_expression,
      $.call_expression,
      $.return_expression,
      $._literal,
      prec.left($.identifier),
      alias(choice(...primitiveTypes), $.identifier),
      // `pushdown` is a keyword only before `fn`; at the start of a statement it would
      // otherwise always lex as one.
      alias('pushdown', $.identifier),
      $.self,
      prec.left($.self_type),
      $.scoped_identifier,
      $.generic_function,
      $.field_expression,
      $.array_expression,
      $.tuple_expression,
      $.unit_expression,
      $.break_expression,
      $.continue_expression,
      $.index_expression,
      $.closure_expression,
      $.parenthesized_expression,
      $.struct_expression,
      $.form_expression,
      $._expression_ending_with_block,
    ),

    _expression: $ => choice(
      $._expression_except_range,
      $.range_expression,
    ),

    _expression_ending_with_block: $ => choice(
      $.block,
      $.if_expression,
      $.match_expression,
      $.while_expression,
      $.loop_expression,
      $.for_expression,
      // Like a block, a `name!{…}` form ends an expression statement.
      alias($._brace_form_expression, $.form_expression),
    ),

    scoped_identifier: $ => seq(
      field('path', choice(
        $._path,
        alias($.generic_type_with_turbofish, $.generic_type),
      )),
      '::',
      field('name', choice($.identifier, $.self_type, 'super')),
    ),

    scoped_type_identifier_in_expression_position: $ => prec(-2, seq(
      field('path', choice(
        $._path,
        alias($.generic_type_with_turbofish, $.generic_type),
      )),
      '::',
      field('name', $._type_identifier),
    )),

    scoped_type_identifier: $ => seq(
      field('path', choice(
        $._path,
        alias($.generic_type_with_turbofish, $.generic_type),
        $.generic_type,
      )),
      '::',
      field('name', $._type_identifier),
    ),

    range_expression: $ => prec.left(PREC.range, choice(
      seq($._expression, choice('..', '..='), $._expression),
      seq($._expression, '..'),
      seq(choice('..', '..='), $._expression),
      '..',
    )),

    unary_expression: $ => prec(PREC.unary, seq(
      choice('-', '*', '!'),
      $._expression,
    )),

    try_expression: $ => prec(PREC.try, seq(
      $._expression,
      '?',
    )),

    reference_expression: $ => prec(PREC.unary, seq(
      '&',
      optional('mut'),
      field('value', $._expression),
    )),

    binary_expression: $ => {
      const table = [
        [PREC.and, '&&'],
        [PREC.or, '||'],
        [PREC.bitand, '&'],
        [PREC.bitor, '|'],
        [PREC.bitxor, '^'],
        [PREC.comparative, choice('==', '!=', '<', '<=', '>', '>=')],
        [PREC.shift, choice('<<', '>>')],
        [PREC.additive, choice('+', '-')],
        [PREC.multiplicative, choice('*', '/', '%')],
      ];

      // @ts-ignore
      return choice(...table.map(([precedence, operator]) => prec.left(precedence, seq(
        field('left', $._expression),
        // @ts-ignore
        field('operator', operator),
        field('right', $._expression),
      ))));
    },

    assignment_expression: $ => prec.right(PREC.assign, seq(
      field('left', $._expression),
      '=',
      field('right', $._expression),
    )),

    compound_assignment_expr: $ => prec.right(PREC.assign, seq(
      field('left', $._expression),
      field('operator', choice('+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '<<=', '>>=')),
      field('right', $._expression),
    )),

    type_cast_expression: $ => prec.left(PREC.cast, seq(
      field('value', $._expression),
      'as',
      field('type', $._type),
    )),

    return_expression: $ => choice(
      prec.left(seq('return', $._expression)),
      prec(-1, 'return'),
    ),

    call_expression: $ => prec(PREC.call, seq(
      field('function', $._expression_except_range),
      field('arguments', $.arguments),
    )),

    arguments: $ => seq(
      '(',
      sepBy(',', $._expression),
      optional(','),
      ')',
    ),

    array_expression: $ => seq(
      '[',
      choice(
        seq($._expression, ';', field('length', $._expression)),
        seq(sepBy(',', $._expression), optional(',')),
      ),
      ']',
    ),

    parenthesized_expression: $ => seq(
      '(',
      $._expression,
      ')',
    ),

    tuple_expression: $ => seq(
      '(',
      seq($._expression, ','),
      repeat(seq($._expression, ',')),
      optional($._expression),
      ')',
    ),

    unit_expression: _ => seq('(', ')'),

    struct_expression: $ => seq(
      field('name', choice(
        $._type_identifier,
        $.self_type,
        alias($.scoped_type_identifier_in_expression_position, $.scoped_type_identifier),
        $.generic_type_with_turbofish,
      )),
      field('body', $.field_initializer_list),
    ),

    field_initializer_list: $ => seq(
      '{',
      sepBy(',', choice(
        $.shorthand_field_initializer,
        $.field_initializer,
      )),
      optional(','),
      '}',
    ),

    shorthand_field_initializer: $ => alias($.identifier, $.shorthand_field_identifier),

    field_initializer: $ => seq(
      field('field', $._field_identifier),
      ':',
      field('value', $._expression),
    ),

    if_expression: $ => prec.right(seq(
      'if',
      field('condition', $._condition),
      field('consequence', $.block),
      optional(field('alternative', $.else_clause)),
    )),

    let_condition: $ => seq(
      'let',
      field('pattern', $._pattern),
      '=',
      field('value', $._expression),
    ),

    _condition: $ => choice(
      $._expression,
      $.let_condition,
    ),

    else_clause: $ => seq(
      'else',
      choice(
        $.block,
        $.if_expression,
      ),
    ),

    match_expression: $ => seq(
      'match',
      field('value', $._expression),
      field('body', $.match_block),
    ),

    match_block: $ => seq(
      '{',
      optional(seq(
        repeat($.match_arm),
        alias($.last_match_arm, $.match_arm),
      )),
      '}',
    ),

    match_arm: $ => prec.right(seq(
      field('pattern', $.match_pattern),
      '=>',
      choice(
        seq(field('value', $._expression), ','),
        field('value', prec(1, $._expression_ending_with_block)),
      ),
    )),

    last_match_arm: $ => seq(
      field('pattern', $.match_pattern),
      '=>',
      field('value', $._expression),
      optional(','),
    ),

    match_pattern: $ => seq(
      $._pattern,
      optional(seq('if', field('condition', $._expression))),
    ),

    while_expression: $ => seq(
      optional(seq($.label, ':')),
      'while',
      field('condition', $._condition),
      field('body', $.block),
    ),

    loop_expression: $ => seq(
      optional(seq($.label, ':')),
      'loop',
      field('body', $.block),
    ),

    for_expression: $ => seq(
      optional(seq($.label, ':')),
      'for',
      field('pattern', $._pattern),
      'in',
      field('value', $._expression),
      field('body', $.block),
    ),

    closure_expression: $ => prec(PREC.closure, seq(
      field('parameters', $.closure_parameters),
      choice(
        seq(
          seq('->', field('return_type', $._type)),
          field('body', $.block),
        ),
        field('body', $._expression),
      ),
    )),

    closure_parameters: $ => choice(
      '||',
      seq(
        '|',
        sepBy(',', choice(
          $._pattern,
          $.parameter,
        )),
        optional(','),
        '|',
      ),
    ),

    break_expression: $ => prec.left(seq('break', optional($.label), optional($._expression))),

    continue_expression: $ => prec.left(seq('continue', optional($.label))),

    index_expression: $ => prec(PREC.call, seq($._expression, '[', $._expression, ']')),

    field_expression: $ => prec(PREC.field, seq(
      field('value', $._expression),
      '.',
      field('field', choice(
        $._field_identifier,
        $.integer_literal,
      )),
    )),

    generic_function: $ => prec(1, seq(
      field('function', choice(
        $.identifier,
        $.scoped_identifier,
        $.field_expression,
      )),
      '::',
      field('type_arguments', $.type_arguments),
    )),

    // ---- forms ------------------------------------------------------------------------------

    form_expression: $ => seq(
      field('name', $.identifier),
      token.immediate('!'),
      choice(
        seq('(', repeat($._form_entry), ')'),
        seq('[', repeat($._form_entry), ']'),
      ),
    ),

    _brace_form_expression: $ => seq(
      field('name', $.identifier),
      token.immediate('!'),
      seq('{', repeat($._form_entry), '}'),
    ),

    _form_entry: $ => choice(
      $.form_attribute,
      $.form_arm,
      prec(-1, $._expression),
      ',',
    ),

    // `key = value` binds tighter than an assignment expression `key = value`.
    form_attribute: $ => seq(
      prec(1, seq(field('name', $.identifier), '=')),
      field('value', $._expression),
    ),

    form_arm: $ => seq(
      field('pattern', $._expression),
      optional(seq('if', field('condition', $._expression))),
      '=>',
      field('value', $._expression),
    ),

    // ---- statements -------------------------------------------------------------------------

    block: $ => seq(
      '{',
      repeat($._statement),
      optional($._expression),
      '}',
    ),

    _statement: $ => choice(
      $.expression_statement,
      $.let_declaration,
      $.empty_statement,
      $._item,
    ),

    empty_statement: _ => ';',

    expression_statement: $ => choice(
      seq($._expression, ';'),
      prec(1, $._expression_ending_with_block),
    ),

    let_declaration: $ => seq(
      'let',
      field('pattern', $._pattern),
      optional(seq(':', field('type', $._type))),
      optional(seq('=', field('value', $._expression))),
      ';',
    ),

    // ---- patterns ---------------------------------------------------------------------------

    _pattern: $ => choice(
      $._literal_pattern,
      alias(choice(...primitiveTypes), $.identifier),
      $.identifier,
      $.self,
      $.scoped_identifier,
      $.tuple_pattern,
      $.tuple_struct_pattern,
      $.struct_pattern,
      $.remaining_field_pattern,
      $.mut_pattern,
      $.range_pattern,
      $.or_pattern,
      '_',
    ),

    tuple_pattern: $ => seq(
      '(',
      sepBy(',', $._pattern),
      optional(','),
      ')',
    ),

    tuple_struct_pattern: $ => seq(
      field('type', choice(
        $.identifier,
        $.self_type,
        $.scoped_identifier,
      )),
      '(',
      sepBy(',', $._pattern),
      optional(','),
      ')',
    ),

    struct_pattern: $ => seq(
      field('type', choice(
        $._type_identifier,
        $.self_type,
        $.scoped_type_identifier,
      )),
      '{',
      sepBy(',', choice($.field_pattern, $.remaining_field_pattern)),
      optional(','),
      '}',
    ),

    field_pattern: $ => seq(
      optional('mut'),
      choice(
        field('name', alias($.identifier, $.shorthand_field_identifier)),
        seq(
          field('name', $._field_identifier),
          ':',
          field('pattern', $._pattern),
        ),
      ),
    ),

    remaining_field_pattern: _ => '..',

    mut_pattern: $ => seq(
      'mut',
      $.identifier,
    ),

    range_pattern: $ => choice(
      seq(
        field('left', choice($._literal_pattern, $._path)),
        choice(
          seq(
            choice('..=', '..'),
            field('right', choice($._literal_pattern, $._path)),
          ),
          '..',
        ),
      ),
      seq(
        '..=',
        field('right', choice($._literal_pattern, $._path)),
      ),
    ),

    or_pattern: $ => prec.left(-2, choice(
      seq($._pattern, '|', $._pattern),
      seq('|', $._pattern),
    )),

    // ---- literals and names -----------------------------------------------------------------

    _literal: $ => choice(
      $.string_literal,
      $.boolean_literal,
      $.integer_literal,
      $.decimal_literal,
    ),

    _literal_pattern: $ => choice(
      $.string_literal,
      $.boolean_literal,
      $.integer_literal,
      $.decimal_literal,
      $.negative_literal,
    ),

    negative_literal: $ => seq('-', choice($.integer_literal, $.decimal_literal)),

    integer_literal: _ => /[0-9][0-9a-zA-Z_]*/,

    decimal_literal: _ => /[0-9][0-9_]*\.[0-9][0-9_]*/,

    string_literal: $ => seq(
      '"',
      repeat(choice(
        $.escape_sequence,
        $._string_content,
      )),
      token.immediate('"'),
    ),

    _string_content: _ => token.immediate(prec(1, /[^"\\]+/)),

    // `\n \r \t \\ \" \0 \u{…}`; other escapes are accepted here and rejected by the compiler.
    escape_sequence: _ => token.immediate(seq(
      '\\',
      choice(
        /u\{[0-9a-fA-F_]*\}/,
        /[^u]/,
        'u',
      ),
    )),

    boolean_literal: _ => choice('true', 'false'),

    line_comment: _ => token(seq('//', /([^/!\n].*|\/\/.*)?/)),

    doc_comment: _ => token(choice(
      /\/\/\/([^/\n].*)?/,
      /\/\/!.*/,
    )),

    block_comment: _ => /\/\*[^*]*\*+([^/*][^*]*\*+)*\//,

    label: _ => /'[_\p{XID_Start}][_\p{XID_Continue}]*/,

    _path: $ => choice(
      $.self,
      $.self_type,
      alias(choice(...primitiveTypes), $.identifier),
      'super',
      'crate',
      $.identifier,
      $.scoped_identifier,
    ),

    identifier: _ => /[_\p{XID_Start}][_\p{XID_Continue}]*/,

    _type_identifier: $ => alias($.identifier, $.type_identifier),
    _field_identifier: $ => alias($.identifier, $.field_identifier),

    self: _ => 'self',
    self_type: _ => 'Self',
  },
});

/**
 * One or more `rule`s separated by `sep`.
 *
 * @param {RuleOrLiteral} sep
 * @param {RuleOrLiteral} rule
 *
 * @returns {SeqRule}
 */
function sepBy1(sep, rule) {
  return seq(rule, repeat(seq(sep, rule)));
}

/**
 * Zero or more `rule`s separated by `sep`.
 *
 * @param {RuleOrLiteral} sep
 * @param {RuleOrLiteral} rule
 *
 * @returns {ChoiceRule}
 */
function sepBy(sep, rule) {
  return optional(sepBy1(sep, rule));
}
