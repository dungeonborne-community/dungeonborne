"use strict";
// Fills dev/fonts.html: the same launcher piece with three font choices (CSS variables of the theme are overridden per
// section, the classes are the launcher's own).
const OPTIONS = [
  ["A. Game font everywhere", "Headings and text — the game's font: Libre Baskerville + Noto Serif for Cyrillic", '"Dungeonborne Serif"', '"Dungeonborne Serif"'],
  ["B. Headings from the game, text in Alegreya Sans", "Headings — the game's font, body text — the current Alegreya Sans", '"Dungeonborne Serif"', '"Alegreya Sans"'],
  ["C. Now: Alegreya SC + Alegreya Sans", "For comparison: what 1.6.0 has", '"Alegreya SC"', '"Alegreya Sans"'],
];
const tpl = document.getElementById("sample");
for (const [label, note, display, sans] of OPTIONS) {
  const node = tpl.content.firstElementChild.cloneNode(true);
  node.querySelector("[data-ref=label]").textContent = label;
  node.querySelector("[data-ref=note]").textContent = note;
  node.style.setProperty("--font-display", `${display}, Georgia, serif`);
  node.style.setProperty("--font-sans", `${sans}, "Segoe UI", sans-serif`);
  node.style.fontFamily = "var(--font-sans)";
  document.getElementById("root").append(node);
}
