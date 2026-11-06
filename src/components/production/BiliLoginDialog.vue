<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import QRCode from 'qrcode'
import { ExternalLink, LoaderCircle, X } from 'lucide-vue-next'

import * as api from '../../lib/api'
import { refreshSettings } from '../../stores/production'
import type { BiliLoginStart } from '../../types'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ close: []; success: [] }>()

const loading = ref(false)
const error = ref('')
const qrDataUrl = ref('')
const statusText = ref('正在生成二维码…')
const login = ref<BiliLoginStart | null>(null)
let pollTimer: number | undefined

function stopPolling(): void {
  window.clearInterval(pollTimer)
  pollTimer = undefined
}

async function startLogin(): Promise<void> {
  stopPolling()
  loading.value = true
  error.value = ''
  qrDataUrl.value = ''
  statusText.value = '正在生成二维码…'
  try {
    login.value = await api.productionBiliLoginStart()
    qrDataUrl.value = await QRCode.toDataURL(login.value.url, {
      width: 320,
      margin: 1,
      color: { dark: '#14171a', light: '#ffffff' },
    })
    statusText.value = '用 B 站 App 扫码'
    pollTimer = window.setInterval(poll, 2200)
  } catch (cause) {
    error.value = api.errorText(cause)
    statusText.value = '二维码获取失败'
  } finally {
    loading.value = false
  }
}

async function poll(): Promise<void> {
  if (!login.value) return
  try {
    const result = await api.productionBiliLoginPoll(login.value.qrcodeKey)
    if (result.status === 'pending') {
      statusText.value = '等待扫码'
      return
    }
    if (result.status === 'scanned') {
      statusText.value = '已扫码，请在手机上确认'
      return
    }
    if (result.status === 'success') {
      stopPolling()
      statusText.value = result.account?.name ? `已登录：${result.account.name}` : '登录成功'
      await refreshSettings()
      emit('success')
      window.setTimeout(() => emit('close'), 450)
      return
    }
    if (result.status === 'expired') {
      stopPolling()
      statusText.value = '二维码已过期'
      return
    }
    if (result.status === 'error') {
      stopPolling()
      error.value = result.message || '登录失败'
    }
  } catch (cause) {
    error.value = api.errorText(cause)
    stopPolling()
  }
}

function close(): void {
  stopPolling()
  emit('close')
}

function openInBrowser(): void {
  if (login.value?.url) void api.openExternalUrl(login.value.url)
}

watch(
  () => props.open,
  (open) => {
    if (open) void startLogin()
    else stopPolling()
  },
  { immediate: true },
)

onBeforeUnmount(stopPolling)
</script>

<template>
  <div v-if="open" class="prod-overlay" @click.self="close">
    <div class="prod-dialog prod-login-dialog" role="dialog" aria-modal="true" aria-labelledby="prodLoginTitle">
      <header class="prod-dialog-head">
        <div>
          <div class="prod-eyebrow">B站账号</div>
          <h2 id="prodLoginTitle">扫码登录</h2>
        </div>
        <button type="button" class="prod-icon-btn" aria-label="关闭登录" @click="close">
          <X :size="15" />
        </button>
      </header>
      <div class="prod-login-body">
        <div class="prod-qr-wrap">
          <div class="prod-qr">
            <img v-if="qrDataUrl" :src="qrDataUrl" alt="B站登录二维码" />
            <LoaderCircle v-else :size="24" :class="{ 'prod-spin': loading }" />
          </div>
          <span>{{ statusText }}</span>
        </div>
        <div class="prod-login-copy">
          <h3>登录后可以做什么</h3>
          <p>从收藏夹、稍后再看和观看历史里直接勾选素材；不登录也可以粘贴公开链接。</p>
          <ul>
            <li>收藏夹按文件夹浏览</li>
            <li>多选视频，一个任务处理多个素材</li>
            <li>登录态只保存在本机</li>
          </ul>
          <button v-if="login?.url" type="button" class="prod-btn" @click="openInBrowser">
            <ExternalLink :size="13" />
            在浏览器打开登录页
          </button>
          <p v-if="error" class="prod-error">{{ error }}</p>
          <p v-if="login?.url" class="prod-login-url">{{ login.url }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.prod-spin { animation: prod-spin 1s linear infinite; }
.prod-error { margin-top: 8px !important; color: var(--red) !important; font-size: 12px !important; }
.prod-login-url {
  margin-top: 8px !important;
  overflow: hidden;
  color: var(--ink-3) !important;
  font-family: var(--mono);
  font-size: 12px !important;
  text-overflow: ellipsis;
  white-space: nowrap;
}
@keyframes prod-spin { to { transform: rotate(360deg); } }
</style>
