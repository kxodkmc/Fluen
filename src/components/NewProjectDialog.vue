<script setup lang="ts">
/**
 * NewProjectDialog — 新建文章对话框。
 *
 * 用户输入文章标题和作者后，在指定存储路径下创建完整的项目文件结构。
 *
 * 功能：
 *   - 文章标题输入 → 自动生成文件夹名预览（可编辑）
 *   - 作者输入
 *   - 存储路径选择（支持手动输入 + 浏览文件夹）
 *   - 完整路径实时预览
 *   - 前端表单校验（非空、路径合法性）
 *   - 调用后端 `create_project` 命令创建文件结构
 *   - 关闭时自动重置表单和错误状态
 *
 * 设计遵循 DESIGN.md：
 *   - 遮罩层 rgba(0,0,0,0.4) + backdrop-filter blur
 *   - 对话框 surface 背景、12px 圆角、card 阴影
 *   - 按钮 Pill 形状，主按钮黑色背景
 *   - 输入框 8px 圆角、hairline 边框
 */
import { ref, computed, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { useI18n } from '../i18n';

const { t } = useI18n();

const props = defineProps<{
  /** 是否显示对话框。 */
  visible: boolean;
}>();

const emit = defineEmits<{
  /** 关闭对话框。 */
  (e: 'close'): void;
  /** 项目创建成功。 */
  (e: 'created', projectPath: string): void;
}>();

/* ── 表单状态 ─────────────────────────────────────────────────────────── */
const title = ref('');
const author = ref('');
const description = ref('');
const folderName = ref('');
const storagePath = ref('');
const isCreating = ref(false);
const errorMessage = ref('');

/** 文件夹名是否被用户手动修改过。 */
const folderNameManuallyEdited = ref(false);

/* ── 文件夹名自动生成 ─────────────────────────────────────────────────── */

/**
 * 将标题清洗为合法的文件夹名（前端预览用，后端双重校验）。
 *
 * 规则：去除非法字符与常见标点，空格转 -，转小写，折叠连续 -，去首尾 -。
 */
function sanitizeTitleToFolderName(raw: string): string {
  // 与后端 INVALID_FILENAME_CHARS 保持一致
  const invalidChars = /[\\/:*?"<>|!.,;~#$%^&()=+[\]{}'-]/g;
  const cleaned = raw
    .replace(invalidChars, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .trim()
    .replace(/^-+|-+$/g, '')
    .toLowerCase();
  return cleaned || 'untitled';
}

/** 标题变化时自动更新文件夹名（除非用户手动修改过）。 */
watch(title, (newTitle) => {
  if (!folderNameManuallyEdited.value) {
    folderName.value = sanitizeTitleToFolderName(newTitle);
  }
});

/** 用户手动编辑文件夹名。 */
function onFolderNameInput(): void {
  folderNameManuallyEdited.value = true;
}

/* ── 完整路径预览 ─────────────────────────────────────────────────────── */
const fullPathPreview = computed(() => {
  if (!storagePath.value || !folderName.value) {
    return '';
  }
  const sep = storagePath.value.includes('\\') ? '\\' : '/';
  const base = storagePath.value.endsWith(sep)
    ? storagePath.value
    : storagePath.value + sep;
  return base + folderName.value + sep;
});

/* ── 路径选择 ─────────────────────────────────────────────────────────── */

/** 浏览文件夹。 */
async function handleBrowse(): Promise<void> {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: storagePath.value || undefined,
    });
    if (typeof selected === 'string' && selected) {
      storagePath.value = selected;
    }
  } catch {
    // 用户取消或出错，保留原有路径
  }
}

/* ── 表单校验 ─────────────────────────────────────────────────────────── */

/** 前端校验，返回错误消息（空字符串表示通过）。 */
function validate(): string {
  if (!title.value.trim()) {
    return t('main.titleBar.newProject.errorTitleRequired');
  }
  if (!author.value.trim()) {
    return t('main.titleBar.newProject.errorAuthorRequired');
  }
  if (!storagePath.value.trim()) {
    return t('main.titleBar.newProject.errorPathRequired');
  }
  return '';
}

/* ── 创建项目 ─────────────────────────────────────────────────────────── */

/** 提交创建。 */
async function handleCreate(): Promise<void> {
  errorMessage.value = '';

  const validationError = validate();
  if (validationError) {
    errorMessage.value = validationError;
    return;
  }

  isCreating.value = true;
  try {
    const projectPath = await invoke<string>('create_project', {
      request: {
        title: title.value.trim(),
        author: author.value.trim(),
        project_name: folderName.value.trim(),
        storage_path: storagePath.value.trim(),
        description: description.value.trim() || null,
      },
    });
    emit('created', projectPath);
    handleClose();
  } catch (err) {
    const msg = typeof err === 'string' ? err : String(err);
    if (msg.includes('已存在')) {
      errorMessage.value = t('main.titleBar.newProject.errorProjectExists');
    } else {
      errorMessage.value = t('main.titleBar.newProject.errorCreateFailed', { message: msg });
    }
  } finally {
    isCreating.value = false;
  }
}

/* ── 关闭与状态重置 ───────────────────────────────────────────────────── */

/** 关闭对话框并重置所有状态。 */
function handleClose(): void {
  emit('close');
}

/** visible 变为 false 时重置表单。 */
watch(
  () => props.visible,
  (newVisible) => {
    if (!newVisible) {
      // 延迟重置，等过渡动画完成
      nextTick(() => {
        title.value = '';
        author.value = '';
        description.value = '';
        folderName.value = '';
        storagePath.value = '';
        errorMessage.value = '';
        folderNameManuallyEdited.value = false;
        isCreating.value = false;
      });
    } else {
      // 打开时获取默认路径
      loadDefaultPath();
    }
  },
);

/** 加载默认存储路径。 */
async function loadDefaultPath(): Promise<void> {
  try {
    const defaultPath = await invoke<string>('get_default_projects_dir');
    storagePath.value = defaultPath;
  } catch {
    // 获取失败，留空让用户手动选择
  }
}

/* ── 键盘交互 ─────────────────────────────────────────────────────────── */

/** 遮罩层点击关闭。 */
function handleOverlayClick(e: MouseEvent): void {
  if (e.target === e.currentTarget) {
    handleClose();
  }
}

/** Escape 键关闭。 */
function handleKeydown(e: KeyboardEvent): void {
  if (e.key === 'Escape' && !isCreating.value) {
    handleClose();
  }
}
</script>

<template>
  <Transition name="overlay">
    <div
      v-if="visible"
      class="dialog-overlay"
      @click="handleOverlayClick"
      @keydown="handleKeydown"
    >
      <Transition name="dialog" appear>
        <div class="dialog" @click.stop>
          <!-- 标题栏 -->
          <div class="dialog__header">
            <h2 class="dialog__title">{{ t('main.titleBar.newProject.title') }}</h2>
            <button
              class="dialog__close"
              :disabled="isCreating"
              @click="handleClose"
            >
              <svg viewBox="0 0 12 12" width="14" height="14">
                <path d="M1 1l10 10M11 1L1 11" stroke="currentColor" stroke-width="1.5" />
              </svg>
            </button>
          </div>

          <!-- 表单区 -->
          <div class="dialog__body">
            <!-- 文章标题 -->
            <div class="field">
              <label class="field__label">{{ t('main.titleBar.newProject.articleTitle') }}</label>
              <input
                v-model="title"
                class="field__input"
                :placeholder="t('main.titleBar.newProject.articleTitlePlaceholder')"
                type="text"
              />
            </div>

            <!-- 作者 -->
            <div class="field">
              <label class="field__label">{{ t('main.titleBar.newProject.author') }}</label>
              <input
                v-model="author"
                class="field__input"
                :placeholder="t('main.titleBar.newProject.authorPlaceholder')"
                type="text"
              />
            </div>

            <!-- 文章描述（可选） -->
            <div class="field">
              <label class="field__label">
                {{ t('main.titleBar.newProject.description') }}
                <span class="field__optional">({{ t('main.titleBar.newProject.optional') }})</span>
              </label>
              <textarea
                v-model="description"
                class="field__textarea"
                :placeholder="t('main.titleBar.newProject.descriptionPlaceholder')"
                rows="3"
              />
            </div>

            <!-- 文件夹名称 -->
            <div class="field">
              <label class="field__label">{{ t('main.titleBar.newProject.folderName') }}</label>
              <input
                v-model="folderName"
                class="field__input"
                type="text"
                @input="onFolderNameInput"
              />
            </div>

            <!-- 存储路径 -->
            <div class="field">
              <label class="field__label">{{ t('main.titleBar.newProject.storagePath') }}</label>
              <div class="field__row">
                <input
                  v-model="storagePath"
                  class="field__input field__input--flex"
                  type="text"
                />
                <button
                  class="field__browse"
                  :disabled="isCreating"
                  @click="handleBrowse"
                >
                  {{ t('main.titleBar.newProject.browse') }}
                </button>
              </div>
            </div>

            <!-- 完整路径预览 -->
            <div v-if="fullPathPreview" class="field">
              <label class="field__label">{{ t('main.titleBar.newProject.pathPreview') }}</label>
              <div class="field__preview">{{ fullPathPreview }}</div>
            </div>

            <!-- 错误信息 -->
            <div v-if="errorMessage" class="dialog__error">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5">
                <circle cx="8" cy="8" r="7" />
                <path d="M8 4v5M8 11v.5" />
              </svg>
              <span>{{ errorMessage }}</span>
            </div>
          </div>

          <!-- 底部按钮 -->
          <div class="dialog__footer">
            <button
              class="btn btn--secondary"
              :disabled="isCreating"
              @click="handleClose"
            >
              {{ t('main.titleBar.newProject.cancel') }}
            </button>
            <button
              class="btn btn--primary"
              :disabled="isCreating"
              @click="handleCreate"
            >
              {{ isCreating ? t('main.titleBar.newProject.creating') : t('main.titleBar.newProject.create') }}
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
}

/* ── 对话框 ─────────────────────────────────────────────────────────── */
.dialog {
  width: 480px;
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
.dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--fluen-hairline);
}

.dialog__title {
  margin: 0;
  font-family: var(--fluen-font-sans);
  font-size: 20px;
  font-weight: 600;
  color: var(--fluen-ink);
}

.dialog__close {
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

.dialog__close:hover:not(:disabled) {
  background: var(--fluen-hover);
  color: var(--fluen-ink);
}

.dialog__close:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ── 表单区 ─────────────────────────────────────────────────────────── */
.dialog__body {
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

/* ── 字段 ───────────────────────────────────────────────────────────── */
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field__label {
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 500;
  color: var(--fluen-slate);
}

.field__input {
  height: 40px;
  padding: 0 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  outline: none;
  transition: border-color 0.15s ease;
}

.field__input:focus {
  border-color: var(--fluen-accent);
}

.field__input::placeholder {
  color: var(--fluen-stone);
}

.field__optional {
  font-weight: 400;
  color: var(--fluen-stone);
  font-size: 12px;
}

.field__textarea {
  padding: 10px 12px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  outline: none;
  resize: vertical;
  min-height: 72px;
  line-height: 1.5;
  transition: border-color 0.15s ease;
}

.field__textarea:focus {
  border-color: var(--fluen-accent);
}

.field__textarea::placeholder {
  color: var(--fluen-stone);
}

.field__row {
  display: flex;
  gap: 8px;
}

.field__input--flex {
  flex: 1;
}

.field__browse {
  flex-shrink: 0;
  height: 40px;
  padding: 0 16px;
  border: 1px solid var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  color: var(--fluen-ink);
  font-family: var(--fluen-font-sans);
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.field__browse:hover:not(:disabled) {
  background: var(--fluen-hover);
  border-color: var(--fluen-accent);
}

.field__browse:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.field__preview {
  padding: 8px 12px;
  border: 1px dashed var(--fluen-hairline);
  border-radius: 8px;
  background: var(--fluen-canvas);
  color: var(--fluen-stone);
  font-family: var(--fluen-font-sans);
  font-size: 13px;
  word-break: break-all;
  line-height: 1.5;
}

/* ── 错误信息 ───────────────────────────────────────────────────────── */
.dialog__error {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  border-radius: 8px;
  background: rgba(212, 86, 86, 0.08);
  color: #d45656;
  font-family: var(--fluen-font-sans);
  font-size: 13px;
}

/* ── 底部按钮 ───────────────────────────────────────────────────────── */
.dialog__footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
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

.btn--primary:hover:not(:disabled) {
  opacity: 0.85;
}

.btn--primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn--secondary {
  background: transparent;
  color: var(--fluen-ink);
  border: 1px solid var(--fluen-hairline);
}

.btn--secondary:hover:not(:disabled) {
  background: var(--fluen-hover);
}

.btn--secondary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
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
