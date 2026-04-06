// Super basic highlight.js for the Cloesce IDL.
hljs.registerLanguage("cloesce", function(hljs) {
    const KEYWORDS = [
      "env","model","source","for","service","inject","api","poo",
      "primary","foreign","unique","keyfield","paginated","optional",
      "d1","r2","kv","vars", "nav"
    ];

    const PRIMITIVES = [
      "string","int","double","date","bool","json","void","blob","stream",
      "R2Object", "self"
    ];

    const GENERICS = [
      "Option","Array","Paginated","KvObject","Partial","DataSource"
    ];


    return {
    name: "cloesce",
    keywords: {
        keyword: KEYWORDS.join(" "),
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
