// Fixture script: intentionally minimal; used to verify asset tracking.
document.addEventListener("DOMContentLoaded", () => {
  document.querySelectorAll("nav a").forEach((link) => {
    link.addEventListener("click", () => {
      window.console.log("fixture navigation", link.getAttribute("href"));
    });
  });
});
