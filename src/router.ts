import { createRouter, createWebHashHistory } from 'vue-router'

import GraphView from './views/GraphView.vue'
import NoteView from './views/NoteView.vue'
import StudioView from './views/StudioView.vue'
import TagsView from './views/TagsView.vue'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/note' },
    { path: '/note/:id?', name: 'note', component: NoteView },
    { path: '/tags', name: 'tags', component: TagsView },
    { path: '/graph', name: 'graph', component: GraphView },
    { path: '/studio', name: 'studio', component: StudioView },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})
