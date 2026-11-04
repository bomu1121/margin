import { createApp } from 'vue'
import './styles/fonts.css'
import './styles/app.css'
import './styles/force.css'
import App from './App.vue'
import { router } from './router'

createApp(App).use(router).mount('#app')
