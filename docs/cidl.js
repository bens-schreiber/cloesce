// Super basic highlight.js for the Cloesce IDL.
hljs.registerLanguage("cloesce", function(hljs) {
    // Top-level reserved words (actual lexer keywords)
    const KEYWORDS = [
      "env","model","source","service","inject","api","poo","sql",
      "d1","r2","kv","vars","self", "internal"
    ];

    // Contextual block keywords
    const BLOCK_KEYWORDS = [
      "primary","foreign","optional","unique","paginated",
      "keyfield","nav","include","for","use"
    ];

    // CRUD / HTTP verbs
    const VERBS = [
      "get","post","put","patch","delete","save","list"
    ];

    const PRIMITIVES = [
      "string","int","double","date","bool","json","void","blob","stream","R2Object"
    ];

    const GENERICS = [
      "Option","Array","Paginated","KvObject","Partial","DataSource"
    ];


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
        end: /"/
        },

        // Punctuation tokens
        {
        className: "punctuation",
        begin: /[{}()[]<>:,.-]/
        }
    ]
    };
});

(function rehighlightCloesceBlocks() {
  if (typeof document === "undefined" || typeof hljs === "undefined") return;

  function applyHighlight() {
    var blocks = document.querySelectorAll("code.language-cloesce");
    blocks.forEach(function(block) {
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
