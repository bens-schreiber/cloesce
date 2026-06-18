hljs.registerLanguage("cloesce", function (hljs) {
  // Top-level declaration keywords
  const KEYWORDS = [
    "env",
    "inject",
    "service",
    "model",
    "api",
    "source",
    "poo",
    "sql",
    "d1",
    "r2",
    "kv",
    "vars",
    "self",
    "durable",
  ];

  // Contextual block / structural keywords
  const BLOCK_KEYWORDS = [
    "route",
    "shard",
    "primary",
    "optional",
    "unique",
    "foreign",
    "nav",
    "column",
    "include",
    "for",
    "crud",
    "use",
    "internal",
    "instance",
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

  // CRUD / HTTP verbs
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

      // Block comments
      hljs.COMMENT("/\\*", "\\*/"),

      // Strings
      {
        className: "string",
        begin: /"/,
        end: /"/,
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
