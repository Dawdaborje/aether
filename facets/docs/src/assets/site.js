// Theme (kept in localStorage; absent storage just means "follow the system") and copy buttons.
(function () {
  var root = document.documentElement;
  try { var saved = localStorage.getItem("aether-docs-theme"); if (saved) root.setAttribute("data-theme", saved); } catch (e) {}
  document.addEventListener("DOMContentLoaded", function () {
    var toggle = document.getElementById("theme-toggle");
    if (toggle) toggle.addEventListener("click", function () {
      var dark = root.getAttribute("data-theme") === "dark" ||
        (!root.getAttribute("data-theme") && window.matchMedia("(prefers-color-scheme: dark)").matches);
      var next = dark ? "light" : "dark";
      root.setAttribute("data-theme", next);
      try { localStorage.setItem("aether-docs-theme", next); } catch (e) {}
    });
    document.querySelectorAll("main pre").forEach(function (pre) {
      var b = document.createElement("button");
      b.className = "copy"; b.type = "button"; b.textContent = "Copy";
      b.addEventListener("click", function () {
        var code = pre.querySelector("code");
        if (navigator.clipboard && code) navigator.clipboard.writeText(code.innerText).then(function () {
          b.textContent = "Copied"; setTimeout(function () { b.textContent = "Copy"; }, 1500);
        });
      });
      pre.appendChild(b);
    });
  });
})();
