hljs.registerLanguage("cloesce", function (hljs) {
  // Top-level block declaration keywords
  const KEYWORDS = [
    "model",
    "poo",
    "source",
    "inject",
    "api",
    "var",
    "d1",
    "r2",
    "kv",
    "durable",
    "self",
  ];

  // Contextual block / structural keywords
  const BLOCK_KEYWORDS = [
    "primary",
    "route",
    "column",
    "foreign",
    "one",
    "many",
    "shard",
    "include",
    "for",
    "crud",
    "internal",
    "instance",
    "header",
    "unique",
    "lt",
    "lte",
    "gt",
    "gte",
    "step",
    "len",
    "minlen",
    "maxlen",
    "regex",
  ];

  // HTTP verbs / data source method kinds
  const VERBS = ["get", "post", "put", "patch", "delete", "list", "save"];

  const PRIMITIVES = [
    "string",
    "int",
    "real",
    "date",
    "bool",
    "json",
    "blob",
    "stream",
    "r2object",
  ];

  const GENERICS = ["option", "array", "kvobject", "partial"];

  return {
    name: "cloesce",
    keywords: {
      keyword: [...KEYWORDS, ...BLOCK_KEYWORDS].join(" "),
      built_in: VERBS.join(" "),
      type: GENERICS.join(" "),
      literal: PRIMITIVES.join(" "),
    },
    contains: [
      // Line comments
      hljs.COMMENT("//", "$"),

      // Strings
      {
        className: "string",
        begin: /"/,
        end: /"/,
      },

      // Pascal_Snake_Case
      {
        className: "symbol",
        begin: /\b(?=[A-Za-z0-9]*[a-z])[A-Z][a-zA-Z0-9]*(?:_[A-Z][a-zA-Z0-9]*)+\b/,
      },

      // SCREAMING_SNAKE_CASE
      {
        className: "variable.constant",
        begin: /\b[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+\b/,
      },

      // PascalCase
      {
        className: "title.class",
        begin: /\b[A-Z][a-zA-Z0-9]*\b/,
      },

      // Punctuation tokens
      {
        className: "punctuation",
        begin: /[{}()[]<>:,.-]/,
      },
    ],
  };
});

(function rehighlightCloesceBlocks() {
  if (typeof document === "undefined" || typeof hljs === "undefined") return;

  function applyHighlight() {
    var blocks = document.querySelectorAll("code.language-cloesce");
    blocks.forEach(function (block) {
      var text = block.textContent;
      block.textContent = text;
      hljs.highlightBlock(block);
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", applyHighlight);
  } else {
    applyHighlight();
  }
})();
