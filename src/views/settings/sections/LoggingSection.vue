<script setup lang="ts">
/**
 * LoggingSection — 日志设置分区。
 *
 * 当前包含：
 * - 日志级别（trace/debug/info/warn/error）
 * - 控制台镜像开关
 * - 日志存放目录（自定义 + 浏览选择）
 * - 单文件最大条目数（512-8192，默认 4096）
 * - 累计日志文件数上限（1-8192，默认 64）
 *
 * 状态由 `useAppConfig` 管理，变更后立即持久化。
 * 日志目录与文件数变更需重启应用后生效（写入器在启动时初始化）。
 */
import { ref, onMounted } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '../../../i18n';
import { useAppConfig } from '../../../composables/useAppConfig';
import type { LogConfig } from '../../../types/app';
import RangeSlider from '../components/RangeSlider.vue';

const { t } = useI18n();
const { loadConfig, saveConfig } = useAppConfig();

/** 单文件最大条目数范围。 */
const MIN_ENTRIES = 512;
const MAX_ENTRIES = 8192;
/** 累计日志文件数范围。 */
const MIN_FILES = 1;
const MAX_FILES = 8192;

/** 日志级别选项。 */
const LEVEL_OPTIONS = ['trace', 'debug', 'info', 'warn', 'error'] as const;

/** 本地状态。 */
const level = ref<string>('info');
const consoleEnabled = ref(false);
const logDir = ref<string>('');
const maxEntries = ref(4096);
const maxFiles = ref(64);
const saving = ref(false);

onMounted(async () => {
  const config = await loadConfig();
  hydrate(config.logging);
});

/** 从 LogConfig 填充本地状态。 */
function hydrate(cfg: LogConfig): void {
  level.value = cfg.level;
  consoleEnabled.value = cfg.console_enabled;
  logDir.value = cfg.log_dir ?? '';
  maxEntries.value = clamp(cfg.max_entries_per_file, MIN_ENTRIES, MAX_ENTRIES, 4096);
  maxFiles.value = clamp(cfg.max_file_count, MIN_FILES, MAX_FILES, 64);
}

/** 数值范围限制。 */
function clamp(value: number, min: number, max: number, fallback: number): number {
  if (Number.isNaN(value)) return fallback;
  return Math.min(max, Math.max(min, Math.floor(value)));
}

/** 读取当前后端日志配置（避免本地未保存的脏数据覆盖）。 */
async function loadCurrentLogging(): Promise<LogConfig> {
  const config = await loadConfig();
  return config.logging;
}

/** 持久化日志配置。 */
async function persistLogging(patch: Partial<LogConfig>): Promise<void> {
  if (saving.value) return;
  saving.value = true;
  try {
    const config = await loadConfig();
    const current = config.logging;
    // 使用 !== undefined 判断，以正确处理 log_dir: null（重置为默认）
    const next: LogConfig = {
      level: patch.level !== undefined ? patch.level : current.level,
      console_enabled:
        patch.console_enabled !== undefined
          ? patch.console_enabled
          : current.console_enabled,
      log_dir: patch.log_dir !== undefined ? patch.log_dir : current.log_dir,
      max_entries_per_file:
        patch.max_entries_per_file !== undefined
          ? patch.max_entries_per_file
          : current.max_entries_per_file,
      max_file_count:
        patch.max_file_count !== undefined
          ? patch.max_file_count
          : current.max_file_count,
    };
    await saveConfig({ ...config, logging: next });
  } catch (err) {
    console.error('[LoggingSection] 保存日志配置失败:', err);
    // 回滚
    const cfg = await loadCurrentLogging();
    hydrate(cfg);
  } finally {
    saving.value = false;
  }
}

/* ── 级别 ─────────────────────────────────────────────────────────── */
async function onLevelChange(event: Event): Promise<void> {
  const target = event.target as HTMLSelectElement;
  level.value = target.value;
  await persistLogging({ level: target.value });
}

/* ── 控制台镜像 ───────────────────────────────────────────────────── */
async function onConsoleToggle(event: Event): Promise<void> {
  const target = event.target as HTMLInputElement;
  consoleEnabled.value = target.checked;
  await persistLogging({ console_enabled: target.checked });
}

/* ── 日志目录 ─────────────────────────────────────────────────────── */
async function onLogDirInput(event: Event): Promise<void> {
  const target = event.target as HTMLInputElement;
  logDir.value = target.value;
}

async function onLogDirBlur(): Promise<void> {
  await persistLogging({ log_dir: logDir.value.trim() || null });
}

async function onBrowseDir(): Promise<void> {
  try {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected === 'string' && selected) {
      logDir.value = selected;
      await persistLogging({ log_dir: selected });
    }
  } catch {
    // 用户取消，静默处理
  }
}

async function onResetDir(): Promise<void> {
  logDir.value = '';
  await persistLogging({ log_dir: null });
}

/* ── 单文件最大条目数 ─────────────────────────────────────────────── */
async function onEntriesChange(): Promise<void> {
  await persistLogging({ max_entries_per_file: maxEntries.value });
}

/* ── 累计文件数 ───────────────────────────────────────────────────── */
async function onFilesChange(): Promise<void> {
  await persistLogging({ max_file_count: maxFiles.value });
}

/* ── 打开日志目录 ─────────────────────────────────────────────────── */
async function onOpenLogsDir(): Promise<void> {
  try {
    await invoke('open_logs_dir');
  } catch (err) {
    console.error('[LoggingSection] 打开日志目录失败:', err);
  }
}
</script>

<template>
  <div class="section">
    <h2 class="section__title">{{ t('settings.sections.logging') }}</h2>
    <p class="section__desc">{{ t('settings.logging.description') }}</p>

    <!-- 日志级别 -->
    <div class="form-group">
      <div class="form-row">
        <label class="form-row__label" for="log-level">
          {{ t('settings.logging.level') }}
        </label>
      </div>
      <select
        id="log-level"
        class="select"
        :value="level"
        :disabled="saving"
        @change="onLevelChange"
      >
        <option v-for="lvl in LEVEL_OPTIONS" :key="lvl" :value="lvl">
          {{ t('settings.logging.levels.' + lvl) }}
        </option>
      </select>
      <p class="form-hint">{{ t('settings.logging.levelHint') }}</p>
    </div>

    <!-- 控制台镜像 -->
    <div class="form-group">
      <div class="form-row form-row--toggle">
        <label class="form-row__label" for="log-console">
          {{ t('settings.logging.console') }}
        </label>
        <label class="switch">
          <input
            id="log-console"
            type="checkbox"
            :checked="consoleEnabled"
            :disabled="saving"
            @change="onConsoleToggle"
          />
          <span class="switch__slider" />
        </label>
      </div>
      <p class="form-hint">{{ t('settings.logging.consoleHint') }}</p>
    </div>

    <!-- 日志存放目录 -->
    <div class="form-group">
      <div class="form-row">
        <label class="form-row__label" for="log-dir">
          {{ t('settings.logging.directory') }}
        </label>
      </div>
      <div class="dir-row">
        <input
          id="log-dir"
          type="text"
          class="input"
          :value="logDir"
          :placeholder="t('settings.logging.directoryPlaceholder')"
          :disabled="saving"
          @input="onLogDirInput"
          @blur="onLogDirBlur"
        />
        <button class="btn" type="button" :disabled="saving" @click="onBrowseDir">
          {{ t('settings.logging.browse') }}
        </button>
        <button class="btn btn--ghost" type="button" :disabled="saving" @click="onResetDir">
          {{ t('settings.logging.reset') }}
        </button>
      </div>
      <p class="form-hint">{{ t('settings.logging.directoryHint') }}</p>
      <button class="btn btn--link" type="button" @click="onOpenLogsDir">
        {{ t('settings.logging.openDir') }}
      </button>
    </div>

    <!-- 单文件最大条目数 -->
    <div class="form-group">
      <div class="form-row">
        <label class="form-row__label" for="log-max-entries">
          {{ t('settings.logging.maxEntries') }}
        </label>
        <span class="form-row__value">{{ maxEntries }}</span>
      </div>
      <RangeSlider
        id="log-max-entries"
        v-model="maxEntries"
        :min="MIN_ENTRIES"
        :max="MAX_ENTRIES"
        :step="128"
        :disabled="saving"
        @change="onEntriesChange"
      />
      <p class="form-hint">{{ t('settings.logging.maxEntriesHint') }}</p>
    </div>

    <!-- 累计日志文件数 -->
    <div class="form-group">
      <div class="form-row">
        <label class="form-row__label" for="log-max-files">
          {{ t('settings.logging.maxFiles') }}
        </label>
        <span class="form-row__value">{{ maxFiles }}</span>
      </div>
      <RangeSlider
        id="log-max-files"
        v-model="maxFiles"
        :min="MIN_FILES"
        :max="MAX_FILES"
        :disabled="saving"
        @change="onFilesChange"
      />
      <p class="form-hint">{{ t('settings.logging.maxFilesHint') }}</p>
    </div>
  </div>
</template>

<style scoped>
.section {
  padding: 0;
}

.section__title {
  margin: 0 0 0.25rem;
  font-family: var(--fluen-font-sans);
  font-size: 1.4rem;
  font-weight: 600;
  color: var(--fluen-ink);
  letter-spacing: -0.5px;
}

.section__desc {
  margin: 0 0 1.5rem;
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  color: var(--fluen-stone);
}

.form-group {
  margin-top: 0.5rem;
  max-width: 480px;
}

.form-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 0.6rem;
}

.form-row--toggle {
  margin-bottom: 0.4rem;
}

.form-row__label {
  font-family: var(--fluen-font-sans);
  font-size: 0.9rem;
  font-weight: 500;
  color: var(--fluen-ink);
}

.form-row__value {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 28px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  background: var(--fluen-info-bg);
  color: var(--fluen-info);
  font-family: var(--fluen-font-mono);
  font-size: 0.85rem;
  font-weight: 600;
}

.form-hint {
  margin: 0.6rem 0 0;
  font-size: 0.72rem;
  color: var(--fluen-stone);
  line-height: 1.4;
}

/* ── 下拉选择 ──────────────────────────────────────────────────────── */
.select {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.85rem;
  cursor: pointer;
  outline: none;
  transition: border-color 0.15s ease;
}

.select:hover,
.select:focus {
  border-color: var(--fluen-accent);
}

.select:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

/* ── 开关 ──────────────────────────────────────────────────────────── */
.switch {
  position: relative;
  display: inline-block;
  width: 36px;
  height: 20px;
  flex-shrink: 0;
}

.switch input {
  opacity: 0;
  width: 0;
  height: 0;
}

.switch__slider {
  position: absolute;
  inset: 0;
  background: var(--fluen-hairline);
  border-radius: 9999px;
  cursor: pointer;
  transition: background 0.2s ease;
}

.switch__slider::before {
  content: '';
  position: absolute;
  left: 2px;
  top: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--fluen-canvas);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.2s ease;
}

.switch input:checked + .switch__slider {
  background: var(--fluen-accent);
}

.switch input:checked + .switch__slider::before {
  transform: translateX(16px);
}

/* ── 目录输入行 ────────────────────────────────────────────────────── */
.dir-row {
  display: flex;
  gap: 6px;
}

.input {
  flex: 1;
  min-width: 0;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-mono);
  font-size: 0.78rem;
  outline: none;
  transition: border-color 0.15s ease;
}

.input:hover,
.input:focus {
  border-color: var(--fluen-accent);
}

.input:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

/* ── 按钮 ──────────────────────────────────────────────────────────── */
.btn {
  height: 32px;
  padding: 0 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 6px;
  background: var(--fluen-surface);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 0.8rem;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.15s ease;
}

.btn:hover:not(:disabled) {
  border-color: var(--fluen-accent);
  color: var(--fluen-accent);
}

.btn:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.btn--ghost {
  background: transparent;
}

.btn--link {
  height: auto;
  padding: 4px 0;
  border: none;
  background: transparent;
  color: var(--fluen-accent);
  font-size: 0.78rem;
  margin-top: 0.4rem;
}

.btn--link:hover:not(:disabled) {
  text-decoration: underline;
}
</style>
