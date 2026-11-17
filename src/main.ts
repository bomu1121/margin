import { createApp } from 'vue'
import './styles/fonts.css'
import './styles/app.css'
import './styles/sidebar.css'
import './styles/force.css'
import './styles/readability.css'
import './styles/facts.css'
import './styles/history.css'
import './styles/studio.css'
import './styles/production.css'
import './styles/panels.css'
import './styles/themes.css'
import './styles/settings.css'
import App from './App.vue'
import { router } from './router'
import { initAppearance } from './stores/appearance'

initAppearance()

createApp(App).use(router).mount('#app')
