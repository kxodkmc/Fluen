<script setup lang="ts">
/**
 * AboutDialog — “关于”对话框。
 *
 * 展示应用名称/版本、Tauri 运行时版本与核心前端依赖库版本。
 * 版本信息打开时懒加载一次（见 utils/aboutInfo），之后复用缓存值。
 *
 * 设计遵循 DESIGN.md：遮罩层 + surface 卡片、12px 圆角、hairline 边框、Pill 按钮。
 */
import { ref, watch } from 'vue';
import { APP_NAME } from '../utils/appInfo';
import {
  getAppVersion,
  getTauriRuntimeVersion,
  getLibraryVersions,
  type VersionEntry,
} from '../utils/aboutInfo';
import { useI18n } from '../i18n';

const { t } = useI18n();

const props = defineProps<{
  /** 是否显示对话框。 */
  visible: boolean;
}>();

const emit = defineEmits<{
  /** 关闭对话框。 */
  (e: 'close'): void;
}>();

/* ── 版本信息（首次打开时加载，之后缓存） ────────────────────────────── */
const appVersion = ref('');
const tauriVersion = ref('');
const libraries = ref<VersionEntry[]>([]);
const loaded = ref(false);

async function loadVersionInfo(): Promise<void> {
  if (loaded.value) return;
  const [app, tauri] = await Promise.all([
    getAppVersion(),
    getTauriRuntimeVersion(),
  ]);
  appVersion.value = app;
  tauriVersion.value = tauri;
  libraries.value = getLibraryVersions();
  loaded.value = true;
}

watch(
  () => props.visible,
  (visible) => {
    if (visible) void loadVersionInfo();
  },
  { immediate: true },
);

/* ── 关闭交互 ─────────────────────────────────────────────────────────── */

/** 遮罩层点击关闭（仅点击遮罩本身）。 */
function handleOverlayClick(e: MouseEvent): void {
  if (e.target === e.currentTarget) {
    emit('close');
  }
}

/** Escape 键关闭。 */
function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape') {
    emit('close');
  }
}
</script>

<template>
  <Transition name="overlay">
    <div
      v-if="visible"
      class="dialog-overlay"
      tabindex="-1"
      @click="handleOverlayClick"
      @keydown="handleKeydown"
    >
      <Transition name="dialog" appear>
        <div class="about-dialog" role="dialog" :aria-label="t('main.about.title')">
          <!-- 标题栏 -->
          <div class="about-dialog__header">
            <h2 class="about-dialog__title">{{ t('main.about.title') }}</h2>
            <button class="about-dialog__close" @click="emit('close')">
              <svg viewBox="0 0 12 12" width="14" height="14">
                <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.5" />
              </svg>
            </button>
          </div>

          <!-- 内容区 -->
          <div class="about-dialog__body">
            <!-- 应用标识 -->
            <div class="about-dialog__hero">
              <svg
                class="about-dialog__logo"
                viewBox="0 0 24 24"
                width="40"
                height="40"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M4 4h12v16H4V4z" />
                <path d="M18 8v12a2 2 0 0 1-2 2" />
                <path d="M8 8h4M8 12h4M8 16h2" />
              </svg>
              <div class="about-dialog__hero-text">
                <span class="about-dialog__name">{{ APP_NAME }}</span>
                <span class="about-dialog__version">
                  v{{ appVersion || t('main.about.loading') }}
                </span>
              </div>
            </div>

            <p class="about-dialog__description">{{ t('main.about.description') }}</p>

            <!-- 版本信息分区 -->
            <section class="about-dialog__section">
              <h3 class="about-dialog__section-title">{{ t('main.about.runtime') }}</h3>
              <div class="version-row">
                <span class="version-row__name">Tauri</span>
                <span class="version-row__value">
                  {{ tauriVersion || t('main.about.loading') }}
                </span>
              </div>
            </section>

            <section class="about-dialog__section">
              <h3 class="about-dialog__section-title">{{ t('main.about.libraries') }}</h3>
              <div v-for="lib in libraries" :key="lib.name" class="version-row">
                <span class="version-row__name">{{ lib.name }}</span>
                <span class="version-row__value">{{ lib.version }}</span>
              </div>
            </section>
          </div>

          <!-- 底部 -->
          <div class="about-dialog__footer">
            <button class="btn btn--primary" @click="emit('close')">
              {{ t('main.about.close') }}
            </button>
          </div>
        </div>
      </Transition>
    </div>
  </Transition>
</template>

<style scoped>
/* ── 遮罩层 ─────────────────────────────────────────────────────────── */
.dialog-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  outline: none;
}

/* ── 对话框 ─────────────────────────────────────────────────────────── */
.about-dialog {
  width: 420px;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 64px);
  overflow-y: auto;
  background: var(--fluen-surface);
  border: 1px solid var(--fluen-hairline);
  border-radius: 12px;
  box-shadow: var(--fluen-shadow-card);
  -webkit-app-region: no-drag;
}

/* ── 标题栏 ─────────────────────────────────────────────────────────── */
.about-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--fluen-hairline);
}

.about-dialog__title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 16px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.about-dialog__close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  color: var(--fluen-stone);
  cursor: pointer;
  border-radius: 4px;
  transition: background 0.15s ease, color 0.15s ease;
}

.about-dialog__close:hover {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

/* ── 内容区 ─────────────────────────────────────────────────────────── */
.about-dialog__body {
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.about-dialog__hero {
  display: flex;
  align-items: center;
  gap: 14px;
}

.about-dialog__logo {
  flex-shrink: 0;
  color: var(--fluen-accent);
}

.about-dialog__hero-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.about-dialog__name {
  font-family: var(--fluen-font-sans);
  font-size: 20px;
  font-weight: 600;
  letter-spacing: -0.3px;
  color: var(--fluen-ink);
}

.about-dialog__version {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  color: var(--fluen-slate);
}

.about-dialog__description {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  line-height: 1.6;
  color: var(--fluen-slate);
}

/* ── 版本分区 ───────────────────────────────────────────────────────── */
.about-dialog__section {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
}

.about-dialog__section-title {
  margin: 0 0 6px;
  font-family: var(--fluen-font-sans);
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.4px;
  color: var(--fluen-stone);
}

.version-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding: 3px 0;
}

.version-row__name {
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  color: var(--fluen-slate);
}

.version-row__value {
  font-family: var(--fluen-font-mono, ui-monospace, monospace);
  font-size: 12px;
  color: var(--fluen-ink);
}

/* ── 底部按钮 ───────────────────────────────────────────────────────── */
.about-dialog__footer {
  display: flex;
  justify-content: flex-end;
  padding: 16px 20px;
  border-top: 1px solid var(--fluen-hairline);
}

.btn {
  height: 38px;
  padding: 0 24px;
  border: none;
  border-radius: 9999px;
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, opacity 0.15s ease;
}

.btn--primary {
  background: var(--fluen-ink);
  color: var(--fluen-on-accent);
}

.btn--primary:hover {
  opacity: 0.85;
}

/* ── 过渡动画 ───────────────────────────────────────────────────────── */
.overlay-enter-active,
.overlay-leave-active {
  transition: opacity 0.2s ease;
}

.overlay-enter-from,
.overlay-leave-to {
  opacity: 0;
}

.dialog-enter-active,
.dialog-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.dialog-enter-from,
.dialog-leave-to {
  opacity: 0;
  transform: scale(0.96) translateY(-8px);
}
</style>
