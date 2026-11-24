import { createApp } from 'vue'

import './styles/fonts.css'
import './styles/app.css'
import './styles/themes.css'
import './styles/whiteboard.css'
import WhiteboardApp from './WhiteboardApp.vue'
import { initAppearance } from './stores/appearance'

initAppearance()
createApp(WhiteboardApp).mount('#whiteboard')
