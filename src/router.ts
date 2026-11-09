import { createRouter, createWebHashHistory } from 'vue-router'

import FactDetailView from './views/FactDetailView.vue'
import FactCurationView from './views/FactCurationView.vue'
import SourceDetailView from './views/SourceDetailView.vue'
import SourceQueueView from './views/SourceQueueView.vue'
import SourcesView from './views/SourcesView.vue'
import FactsView from './views/FactsView.vue'
import GraphView from './views/GraphView.vue'
import NoteView from './views/NoteView.vue'
import ProductionView from './views/ProductionView.vue'
import TagsView from './views/TagsView.vue'

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', redirect: '/note' },
    { path: '/note/:id?', name: 'note', component: NoteView },
    { path: '/tags', name: 'tags', component: TagsView },
    { path: '/graph', name: 'graph', component: GraphView },
    { path: '/facts', name: 'facts', component: FactsView },
    { path: '/facts/curate', name: 'fact-curation', component: FactCurationView },
    { path: '/facts/:id', name: 'fact', component: FactDetailView },
    { path: '/sources', name: 'sources', component: SourcesView },
    { path: '/sources/pending', name: 'source-queue', component: SourceQueueView },
    { path: '/sources/:id', name: 'source', component: SourceDetailView },
    { path: '/studio', name: 'studio', component: ProductionView },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})
