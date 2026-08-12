(function () {
  "use strict";
  var input = document.getElementById("site-search");
  var output = document.getElementById("search-results");
  if (!input || !output) return;
  var index = window.elasticlunr && window.searchIndex
    ? window.elasticlunr.Index.load(window.searchIndex)
    : null;
  var documents = window.searchIndex && window.searchIndex.documentStore
    ? window.searchIndex.documentStore.docs
    : {};
  function render() {
    var query = input.value.trim().toLowerCase();
    if (!query) { output.innerHTML = "<p class=\"muted\">Type to search.</p>"; return; }
    var matches = index ? index.search(query, { expand: true }).slice(0, 30).map(function (hit) {
      return documents[hit.ref];
    }).filter(Boolean) : [];
    if (!matches.length) { output.innerHTML = "<p class=\"muted\">No matching documentation.</p>"; return; }
    output.innerHTML = matches.map(function (entry) {
      var title = String(entry.title || "Untitled").replace(/[&<>\"]/g, "");
      var description = String(entry.description || "").replace(/[&<>\"]/g, "");
      var url = String(entry.permalink || "#");
      return "<a class=\"search-result\" href=\"" + url + "\"><strong>" + title + "</strong><span>" + description + "</span></a>";
    }).join("");
  }
  input.addEventListener("input", render);
  render();
}());
