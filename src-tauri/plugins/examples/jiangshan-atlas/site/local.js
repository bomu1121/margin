/* 页边本地补丁（P240）：面板顶部的「显示」开关通过 postMessage 控制地图内
   非刚需 UI。默认在 index.html 的 <html> 上挂 yb-hide-* 类，先隐藏再等宿主
   指令，避免打开时闪一下完整界面。
   只加/删类名，不复制上游的 display 值；关闭时靠 !important 隐藏，
   打开时移除类，完全回到上游自己的布局。 */
(() => {
  const root = document.documentElement
  const keys = ['timeline', 'card', 'legend', 'badge']

  function apply(state) {
    if (!state) return
    for (const key of keys) {
      // 未传的键保持当前状态，避免旧版宿主把新加的开关误关掉。
      if (!(key in state)) continue
      root.classList.toggle(`yb-hide-${key}`, !state[key])
    }
  }

  window.addEventListener('message', (event) => {
    const data = event.data
    if (!data || data.type !== 'yebian:atlas-ui') return
    apply(data.state)
  })
})()
