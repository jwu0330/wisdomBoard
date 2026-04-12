/**
 * 統一的 Tauri 初始化輪詢（overlay.html、panel.html 共用）
 * settings.html 使用 ES module，不需要此檔案。
 *
 * 用法：
 *   <script src="/tauri-init.js"></script>
 *   <script>
 *     waitForTauri(function(ctx) {
 *       // ctx.invoke, ctx.win, ctx.listen 可用
 *     });
 *   </script>
 *
 * 注意：此檔案放在 public/ 目錄，確保 Vite build 時複製到 dist/ 根目錄，
 * 並以絕對路徑 /tauri-init.js 引用，避免多頁面 HTML 的相對路徑問題。
 */
function waitForTauri(callback, maxAttempts) {
  var count = 0;
  maxAttempts = maxAttempts || 50;
  function check() {
    if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
      callback({
        invoke: window.__TAURI__.core.invoke,
        win: window.__TAURI__.window.getCurrentWindow(),
        listen: window.__TAURI__.event.listen
      });
    } else if (count++ < maxAttempts) {
      setTimeout(check, 100);
    } else {
      // 超時：顯示在頁面上（若有 hint 元素）或 console
      var hint = document.getElementById('hint');
      if (hint) {
        hint.innerHTML = 'Tauri 初始化超時，請重試<small>按 ESC 取消</small>';
      }
      console.error('[WisdomBoard] waitForTauri: window.__TAURI__ 在 5 秒內未就緒');
    }
  }
  check();
}
