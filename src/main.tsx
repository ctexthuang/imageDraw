import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import ReactDOM from 'react-dom/client';
import {
  CloudDownloadOutlined,
  FolderOpenOutlined,
  PictureOutlined,
  RobotOutlined,
  SettingOutlined,
  StopOutlined,
  SyncOutlined,
  VerticalAlignTopOutlined,
} from '@ant-design/icons';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import appLogo from './assets/logo.svg';
import './styles.css';

type ProviderConfig = {
  id: string;
  name: string;
  kind: string;
  base_url: string;
  api_key?: string | null;
  text_model?: string | null;
  image_model?: string | null;
  capabilities?: string | null;
  enabled: boolean;
};

type ProviderForm = {
  id: string;
  name: string;
  kind: string;
  base_url: string;
  api_key: string;
  text_model?: string | null;
  image_model?: string | null;
  enabled: boolean;
};

type GenerateImageOutput = {
  task: {
    id: string;
    status: string;
  };
  asset: {
    id: string;
    file_path: string;
    display_path?: string | null;
  };
};

type SessionImage = {
  id: string;
  file_path: string;
  display_path?: string | null;
  prompt: string;
  created_at: string;
};

type ProviderModel = {
  id: string;
  owned_by?: string | null;
};

type ProviderCapabilities = {
  image_models?: ProviderModel[];
  selected_image_models?: string[];
};

type GenerationStep = {
  label: string;
  status: 'pending' | 'active' | 'done' | 'error';
};

type UpdateInfo = {
  current_version: string;
  latest_version: string;
  latest_tag: string;
  release_name: string;
  release_url: string;
  release_notes: string;
  published_at?: string | null;
  has_update: boolean;
  asset?: {
    name: string;
    download_url: string;
  } | null;
};

type UpdateDownloadInfo = {
  file_path: string;
};

type UpdateDownloadProgress = {
  downloaded_bytes: number;
  file_name: string;
  total_bytes?: number | null;
};

type GalleryDirectoryInfo = {
  directory: string;
  is_custom: boolean;
};

type SetGalleryDirectoryOutput = {
  directory: GalleryDirectoryInfo;
  moved_paths: Array<{
    old_path: string;
    new_path: string;
    display_path?: string | null;
  }>;
};

function formatError(error: unknown) {
  const message =
    typeof error === 'string' ? error : error instanceof Error ? error.message : JSON.stringify(error);
  if (message.includes('生成已强制停止')) {
    return '生成已强制停止';
  }
  if (message.includes('502 Bad Gateway') || message.includes('upstream_error')) {
    return '上游模型服务返回 502。通常是中转站或模型供应商临时失败，不是本地程序错误；可以换模型、降低分辨率，或稍后/换供应商重试。';
  }
  if (message.includes('503') || message.includes('504')) {
    return '上游模型服务暂时不可用或超时。可以稍后重试，或切换模型/供应商。';
  }
  return message;
}

function isErrorStatus(message: string) {
  return message.includes('失败') || message.includes('为空') || message.startsWith('请先');
}

function imageDisplayPath(image: Pick<SessionImage, 'file_path' | 'display_path'>) {
  return image.display_path || image.file_path;
}

function formatBytes(value?: number | null) {
  if (!value || value <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  const precision = unitIndex === 0 ? 0 : size >= 10 ? 1 : 2;
  return `${size.toFixed(precision)} ${units[unitIndex]}`;
}

function parseProviderCapabilities(value?: string | null): ProviderCapabilities {
  if (!value) return {};
  try {
    return JSON.parse(value) as ProviderCapabilities;
  } catch {
    return {};
  }
}

function buildProviderCapabilities(models: ProviderModel[], selectedModels: string[]) {
  return JSON.stringify({
    responses_api: true,
    images_api: true,
    chat_completions: true,
    image_edit: true,
    image_models: models,
    selected_image_models: selectedModels,
  });
}

const defaultProviderForm: ProviderForm = {
  id: 'default-openai',
  name: 'OpenAI / 中转站',
  kind: 'openai-compatible',
  base_url: 'https://api.openai.com/v1',
  api_key: '',
  text_model: null,
  image_model: null,
  enabled: true,
};

const apiKindOptions = [
  {
    value: 'openai',
    label: 'OpenAI 官方',
    sampleId: 'openai',
    sampleName: 'OpenAI 官方',
    baseUrl: 'https://api.openai.com/v1',
    supported: true,
  },
  {
    value: 'openai-compatible',
    label: 'OpenAI-compatible / 中转站',
    sampleId: 'openai-compatible',
    sampleName: 'OpenAI-compatible / 中转站',
    baseUrl: 'https://api.openai.com/v1',
    supported: true,
  },
  {
    value: 'volcengine-ark',
    label: '火山方舟 / Seedream',
    sampleId: 'volcengine-seedream',
    sampleName: '火山方舟 / Seedream',
    baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
    supported: true,
  },
  {
    value: 'dashscope',
    label: '阿里云百炼 / 通义万相',
    sampleId: 'dashscope-image',
    sampleName: '阿里云百炼 / 通义万相',
    baseUrl: 'https://dashscope.aliyuncs.com/api/v1',
    supported: true,
  },
  {
    value: 'tencent-hunyuan',
    label: '腾讯混元图像',
    sampleId: 'tencent-hunyuan-image',
    sampleName: '腾讯混元图像',
    baseUrl: 'https://aiart.tencentcloudapi.com',
    supported: true,
  },
  {
    value: 'google-gemini',
    label: 'Google Gemini / Nano Banana',
    sampleId: 'google-nano-banana',
    sampleName: 'Google Gemini / Nano Banana',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta',
    supported: true,
  },
  { value: 'stability-ai', label: 'Stability AI（待接入）', supported: false },
  { value: 'replicate', label: 'Replicate（待接入）', supported: false },
  { value: 'fal-ai', label: 'fal.ai（待接入）', supported: false },
];

const initialGenerationSteps: GenerationStep[] = [
  { label: '检查配置', status: 'pending' },
  { label: '提交任务', status: 'pending' },
  { label: '等待模型返回', status: 'pending' },
  { label: '保存到应用文件夹', status: 'pending' },
  { label: '更新结果列表', status: 'pending' },
];

const defaultImageModelOptions: string[] = [];
const imageCountOptions = [1, 2, 3, 4, 5];
const imageSizeOptions = [
  { value: 'auto', label: 'auto', aspectRatio: 'auto', shape: 'auto' },
  { value: '1024x1024', label: '1024x1024', aspectRatio: '1:1', shape: 'square' },
  { value: '1536x1024', label: '1536x1024', aspectRatio: '3:2', shape: 'landscape' },
  { value: '1024x1536', label: '1024x1536', aspectRatio: '2:3', shape: 'portrait' },
  { value: '2048x2048', label: '2048x2048', aspectRatio: '1:1', shape: 'square' },
  { value: '2048x1152', label: '2048x1152', aspectRatio: '16:9', shape: 'wide' },
  { value: '3840x2160', label: '3840x2160', aspectRatio: '16:9', shape: 'wide' },
  { value: '2160x3840', label: '2160x3840', aspectRatio: '9:16', shape: 'tall' },
];

function isTauriRuntime() {
  return typeof window !== 'undefined'
    && Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

function getAspectRatioForSize(value: string) {
  return imageSizeOptions.find((option) => option.value === value)?.aspectRatio ?? 'auto';
}

function normalizeImageSize(value: string) {
  return imageSizeOptions.some((option) => option.value === value) ? value : imageSizeOptions[0].value;
}

function newestFirstModels(models: ProviderModel[]) {
  return [...models].reverse();
}

function modelIds(models: ProviderModel[]) {
  return models.map((model) => model.id);
}

function selectedModelIdsInOrder(orderedIds: string[], selectedIds?: string[]) {
  if (!selectedIds || selectedIds.length === 0) {
    return orderedIds;
  }
  const selected = new Set(selectedIds);
  return orderedIds.filter((id) => selected.has(id));
}

function selectedOrDefaultModel(orderedIds: string[], selectedId: string) {
  return orderedIds.includes(selectedId) ? selectedId : (orderedIds[0] ?? '');
}

const referenceMentionPattern = /@参考图\s*([1-9]\d*)/g;

type ReferenceMentionRange = {
  end: number;
  index: number;
  start: number;
  text: string;
};

type PromptHighlightPart = {
  isReferenceMention: boolean;
  text: string;
};

type ReferenceMentionPopoverPosition = {
  left: number;
  maxHeight: number;
  top: number;
  width: number;
};

function referenceMentionIndexes(value: string) {
  return Array.from(value.matchAll(referenceMentionPattern), (match) => Number(match[1]));
}

function referenceMentionRanges(value: string): ReferenceMentionRange[] {
  return Array.from(value.matchAll(referenceMentionPattern), (match) => {
    const start = match.index ?? 0;
    return {
      end: start + match[0].length,
      index: Number(match[1]),
      start,
      text: match[0],
    };
  });
}

function referenceMentionCounts(value: string) {
  const counts = new Map<number, number>();
  referenceMentionIndexes(value).forEach((index) => {
    counts.set(index, (counts.get(index) ?? 0) + 1);
  });
  return counts;
}

function removedReferenceMentionIndexes(previousValue: string, nextValue: string) {
  const previousCounts = referenceMentionCounts(previousValue);
  const nextCounts = referenceMentionCounts(nextValue);
  return Array.from(previousCounts.keys())
    .filter((index) => (nextCounts.get(index) ?? 0) === 0)
    .sort((a, b) => a - b);
}

function rewriteReferenceMentionsAfterRemovedIndexes(value: string, removedIndexes: number[]) {
  if (removedIndexes.length === 0) return value;

  const sortedRemovedIndexes = Array.from(new Set(removedIndexes)).sort((a, b) => a - b);
  const removedIndexSet = new Set(sortedRemovedIndexes);
  return value.replace(referenceMentionPattern, (_match, index) => {
    const originalIndex = Number(index);
    if (removedIndexSet.has(originalIndex)) return '';

    const shift = sortedRemovedIndexes.filter((removedIndex) => removedIndex < originalIndex).length;
    return `@参考图${originalIndex - shift}`;
  });
}

function promptHighlightParts(value: string, referenceCount: number): PromptHighlightPart[] {
  const ranges = referenceMentionRanges(value);
  const parts: PromptHighlightPart[] = [];
  let cursor = 0;

  ranges.forEach((range) => {
    if (range.start > cursor) {
      parts.push({ isReferenceMention: false, text: value.slice(cursor, range.start) });
    }
    parts.push({ isReferenceMention: range.index <= referenceCount, text: range.text });
    cursor = range.end;
  });

  if (cursor < value.length) {
    parts.push({ isReferenceMention: false, text: value.slice(cursor) });
  }

  return parts.length > 0 ? parts : [{ isReferenceMention: false, text: ' ' }];
}

function linkedReferenceDeletionRange(
  value: string,
  start: number,
  end: number,
  key: string,
) {
  const ranges = referenceMentionRanges(value);
  if (start === end) {
    return ranges.find((range) =>
      key === 'Backspace'
        ? range.start < start && start <= range.end
        : range.start <= start && start < range.end,
    ) ?? null;
  }

  let nextStart = start;
  let nextEnd = end;
  let changed = true;
  while (changed) {
    changed = false;
    ranges.forEach((range) => {
      const overlaps = range.start < nextEnd && range.end > nextStart;
      if (!overlaps) return;
      if (range.start < nextStart) {
        nextStart = range.start;
        changed = true;
      }
      if (range.end > nextEnd) {
        nextEnd = range.end;
        changed = true;
      }
    });
  }

  return nextStart !== start || nextEnd !== end
    ? { end: nextEnd, index: 0, start: nextStart, text: value.slice(nextStart, nextEnd) }
    : null;
}

function invalidReferenceMentionIndex(value: string, referenceCount: number) {
  return referenceMentionIndexes(value).find((index) => index > referenceCount);
}

function buildPromptForReferenceImages(value: string, referenceCount: number) {
  if (referenceCount === 0 || referenceMentionIndexes(value).length === 0) {
    return value;
  }

  const normalizedPrompt = value.replace(referenceMentionPattern, (_match, index) => `Image ${Number(index)}`);
  const referenceLines = Array.from(
    { length: referenceCount },
    (_item, index) => `Image ${index + 1}: attached reference image ${index + 1}.`,
  );

  return [
    'Reference images are attached in this exact order:',
    ...referenceLines,
    'When the user instruction mentions Image N, use the Nth attached reference image.',
    '',
    'User instruction:',
    normalizedPrompt,
  ].join('\n');
}

function activeReferenceMention(value: string, cursor: number) {
  const beforeCursor = value.slice(0, cursor);
  const start = beforeCursor.lastIndexOf('@');
  if (start === -1) return null;

  const query = beforeCursor.slice(start + 1);
  if (query.includes('@') || /\s/.test(query)) return null;
  if (query && !'参考图'.startsWith(query) && !/^参考图[1-9]\d*$/.test(query)) return null;

  return { start, end: cursor, query };
}

function apiKeyPlaceholder(kind: string) {
  if (kind === 'tencent-hunyuan') return 'SecretId:SecretKey';
  if (kind === 'google-gemini') return 'Google AI Studio API Key';
  if (kind === 'dashscope') return 'sk-... 或阿里云百炼 API Key';
  return 'sk-... 或中转站 key';
}

function providerSettingsTip(kind: string) {
  if (kind === 'tencent-hunyuan') {
    return '腾讯云使用 API 3.0 签名，API Key 填 SecretId:SecretKey。';
  }
  if (kind === 'dashscope') {
    return '阿里云百炼 Base URL 默认即可，API Key 填 DashScope Key。';
  }
  if (kind === 'google-gemini') {
    return 'Google Gemini 图像模型也叫 Nano Banana，API Key 填 Google AI Studio Key。';
  }
  return 'Base URL 填 API 地址，通常以 /v1 结尾。';
}

function createRequestId() {
  return window.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

const materialImageExtensions = new Set(['png', 'jpg', 'jpeg', 'webp']);

function isMaterialImagePath(path: string) {
  const cleanPath = path.split(/[?#]/)[0] ?? path;
  const extension = cleanPath.split('.').pop()?.toLowerCase();
  return extension ? materialImageExtensions.has(extension) : false;
}

function fileUriToPath(value: string) {
  try {
    const url = new URL(value);
    if (url.protocol !== 'file:') return '';
    let path = decodeURIComponent(url.pathname);
    if (/^\/[A-Za-z]:\//.test(path)) {
      path = path.slice(1);
    }
    return path;
  } catch {
    return '';
  }
}

function droppedPathsFromDataTransfer(dataTransfer: DataTransfer) {
  const paths = Array.from(dataTransfer.files)
    .map((file) => (file as File & { path?: string }).path ?? '')
    .filter(Boolean);
  const uriList = dataTransfer
    .getData('text/uri-list')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('#'))
    .map(fileUriToPath)
    .filter(Boolean);
  const plainTextPaths = dataTransfer
    .getData('text/plain')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .map((line) => (line.startsWith('file:') ? fileUriToPath(line) : line))
    .filter(Boolean);

  return Array.from(new Set([...paths, ...uriList, ...plainTextPaths]));
}

function clientPointFromDragPosition(position: {
  toLogical?: (scaleFactor: number) => { x: number; y: number };
  x: number;
  y: number;
}) {
  return position.toLogical?.(window.devicePixelRatio || 1) ?? position;
}

function isPointInsideElement(element: HTMLElement | null, x: number, y: number) {
  if (!element) return false;
  const rect = element.getBoundingClientRect();
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

function App() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [providerForm, setProviderForm] = useState<ProviderForm>(defaultProviderForm);
  const [prompt, setPrompt] = useState('一只赛博朋克风格的橘猫坐在霓虹灯下');
  const [selectedImageModel, setSelectedImageModel] = useState('');
  const [fetchedImageModels, setFetchedImageModels] = useState<ProviderModel[]>([]);
  const [selectedImageModels, setSelectedImageModels] = useState<string[]>(defaultImageModelOptions);
  const [imageSize, setImageSize] = useState('auto');
  const [imageCount, setImageCount] = useState(1);
  const [status, setStatus] = useState('准备就绪');
  const [settingsStatus, setSettingsStatus] = useState('');
  const [isBusy, setIsBusy] = useState(false);
  const [activeGenerationRequestIds, setActiveGenerationRequestIds] = useState<string[]>([]);
  const [isFetchingModels, setIsFetchingModels] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isModelPickerOpen, setIsModelPickerOpen] = useState(false);
  const [isCountPickerOpen, setIsCountPickerOpen] = useState(false);
  const [isSizePickerOpen, setIsSizePickerOpen] = useState(false);
  const [isGalleryOpen, setIsGalleryOpen] = useState(false);
  const [isUpdatingGalleryDirectory, setIsUpdatingGalleryDirectory] = useState(false);
  const [galleryInfo, setGalleryInfo] = useState<GalleryDirectoryInfo | null>(null);
  const [galleryStatus, setGalleryStatus] = useState('');
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [isDownloadingUpdate, setIsDownloadingUpdate] = useState(false);
  const [isUpdateDownloadPaused, setIsUpdateDownloadPaused] = useState(false);
  const [isUpdateOpen, setIsUpdateOpen] = useState(false);
  const autoUpdateDismissedRef = useRef(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateDownloadProgress, setUpdateDownloadProgress] = useState<UpdateDownloadProgress | null>(null);
  const [updateStatus, setUpdateStatus] = useState('');
  const paramsRef = useRef<HTMLDivElement | null>(null);
  const materialDragOriginRef = useRef<{ x: number; y: number } | null>(null);
  const materialCardRefs = useRef(new Map<string, HTMLElement>());
  const materialDropZoneRef = useRef<HTMLDivElement | null>(null);
  const materialDropHoverRef = useRef(false);
  const promptInputWrapRef = useRef<HTMLDivElement | null>(null);
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);
  const [previewImage, setPreviewImage] = useState<SessionImage | null>(null);
  const [sessionImages, setSessionImages] = useState<SessionImage[]>([]);
  const [materialPaths, setMaterialPaths] = useState<string[]>([]);
  const [draggedMaterialPath, setDraggedMaterialPath] = useState<string | null>(null);
  const [dragOverMaterialPath, setDragOverMaterialPath] = useState<string | null>(null);
  const [materialDragOffset, setMaterialDragOffset] = useState({ x: 0, y: 0 });
  const [isMaterialDropActive, setIsMaterialDropActive] = useState(false);
  const [promptScrollTop, setPromptScrollTop] = useState(0);
  const [referenceMentionRange, setReferenceMentionRange] = useState<ReturnType<typeof activeReferenceMention>>(null);
  const [referenceMentionPopoverPosition, setReferenceMentionPopoverPosition] =
    useState<ReferenceMentionPopoverPosition | null>(null);
  const [activeReferenceMentionOptionIndex, setActiveReferenceMentionOptionIndex] = useState(0);
  const [generationSteps, setGenerationSteps] = useState<GenerationStep[]>(initialGenerationSteps);
  const imageAspectRatio = getAspectRatioForSize(imageSize);
  const selectedSizeOption =
    imageSizeOptions.find((option) => option.value === imageSize) ?? imageSizeOptions[0];
  const visibleImageModelOptions =
    selectedImageModels.length > 0 ? selectedImageModels : defaultImageModelOptions;
  const activeProviderName =
    providers.find((provider) => provider.id === providerForm.id)?.name ?? providerForm.name;
  const activeMode = materialPaths.length > 0 ? '图像编辑' : '文字生成';
  const referenceMentionOptions = materialPaths
    .map((path, index) => ({ path, index: index + 1 }))
    .filter(({ index }) => {
      const query = referenceMentionRange?.query ?? '';
      return query === '' || `参考图${index}`.startsWith(query);
    });
  const activeReferenceMentionOption =
    referenceMentionOptions[activeReferenceMentionOptionIndex] ?? referenceMentionOptions[0];
  const updateDownloadPercent =
    updateDownloadProgress?.total_bytes
      ? Math.min(
          100,
          Math.round((updateDownloadProgress.downloaded_bytes / updateDownloadProgress.total_bytes) * 100),
        )
      : null;

  function setStep(index: number, status: GenerationStep['status']) {
    setGenerationSteps((steps) =>
      steps.map((step, stepIndex) => (stepIndex === index ? { ...step, status } : step)),
    );
  }

  function startStep(index: number) {
    setGenerationSteps((steps) =>
      steps.map((step, stepIndex) => {
        if (stepIndex < index) return { ...step, status: 'done' };
        if (stepIndex === index) return { ...step, status: 'active' };
        return { ...step, status: 'pending' };
      }),
    );
  }

  function closeParamPickers() {
    setIsModelPickerOpen(false);
    setIsCountPickerOpen(false);
    setIsSizePickerOpen(false);
  }

  function updatePrompt(value: string, cursor: number) {
    let nextPrompt = value;
    const promptMentionCount = referenceMentionIndexes(prompt).length;
    const nextMentionCount = referenceMentionIndexes(value).length;
    const removedIndexes =
      nextMentionCount < promptMentionCount
        ? removedReferenceMentionIndexes(prompt, value).filter((index) => index <= materialPaths.length)
        : [];

    if (removedIndexes.length > 0) {
      const removedIndexSet = new Set(removedIndexes);
      const removedPaths = materialPaths.filter((_path, index) => removedIndexSet.has(index + 1));
      setMaterialPaths((current) => current.filter((_path, index) => !removedIndexSet.has(index + 1)));
      cleanupMaterialImages(removedPaths);
      nextPrompt = rewriteReferenceMentionsAfterRemovedIndexes(value, removedIndexes);
    }

    const nextCursor = Math.min(cursor, nextPrompt.length);
    setPrompt(nextPrompt);
    setReferenceMentionRange(activeReferenceMention(nextPrompt, nextCursor));
    if (nextPrompt !== value) {
      window.requestAnimationFrame(() => {
        promptTextareaRef.current?.setSelectionRange(nextCursor, nextCursor);
      });
    }
  }

  function handleReferenceMentionKeyboard(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (!referenceMentionRange || referenceMentionOptions.length === 0) return false;

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActiveReferenceMentionOptionIndex((current) => (current + 1) % referenceMentionOptions.length);
      return true;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActiveReferenceMentionOptionIndex((current) =>
        (current - 1 + referenceMentionOptions.length) % referenceMentionOptions.length,
      );
      return true;
    }

    if (event.key === 'Enter' || event.key === 'Tab') {
      event.preventDefault();
      if (activeReferenceMentionOption) {
        chooseReferenceMention(activeReferenceMentionOption.index);
      }
      return true;
    }

    if (event.key === 'Escape') {
      event.preventDefault();
      setReferenceMentionRange(null);
      return true;
    }

    return false;
  }

  function deleteLinkedReferenceMention(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== 'Backspace' && event.key !== 'Delete') return;

    const textarea = event.currentTarget;
    const deletionRange = linkedReferenceDeletionRange(
      prompt,
      textarea.selectionStart,
      textarea.selectionEnd,
      event.key,
    );
    if (!deletionRange) return;

    event.preventDefault();
    const nextPrompt = `${prompt.slice(0, deletionRange.start)}${prompt.slice(deletionRange.end)}`;
    updatePrompt(nextPrompt, deletionRange.start);
    window.requestAnimationFrame(() => {
      promptTextareaRef.current?.setSelectionRange(deletionRange.start, deletionRange.start);
    });
  }

  function handlePromptKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (handleReferenceMentionKeyboard(event)) return;
    deleteLinkedReferenceMention(event);
  }

  function refreshReferenceMention() {
    const textarea = promptTextareaRef.current;
    if (!textarea) return;
    setReferenceMentionRange(activeReferenceMention(textarea.value, textarea.selectionStart));
  }

  function chooseReferenceMention(index: number) {
    const mention = `@参考图${index}`;
    const textarea = promptTextareaRef.current;
    const start = referenceMentionRange?.start ?? textarea?.selectionStart ?? prompt.length;
    const end = referenceMentionRange?.end ?? textarea?.selectionEnd ?? start;
    const nextPrompt = `${prompt.slice(0, start)}${mention}${prompt.slice(end)}`;
    const nextCursor = start + mention.length;
    setPrompt(nextPrompt);
    setReferenceMentionRange(null);
    window.requestAnimationFrame(() => {
      promptTextareaRef.current?.focus();
      promptTextareaRef.current?.setSelectionRange(nextCursor, nextCursor);
    });
  }

  function updateProviderForm<K extends keyof ProviderForm>(key: K, value: ProviderForm[K]) {
    setProviderForm((current) => ({ ...current, [key]: value }));
  }

  function updateProviderKind(kind: string) {
    const option = apiKindOptions.find((item) => item.value === kind);
    closeParamPickers();
    setFetchedImageModels([]);
    setSelectedImageModels([]);
    setSelectedImageModel('');
    setSettingsStatus('');
    setProviderForm((current) => ({
      ...defaultProviderForm,
      api_key: '',
      base_url: option?.baseUrl || '',
      id: option?.sampleId || `${kind}-provider`,
      kind,
      name: option?.sampleName || option?.label || current.name,
    }));
  }

  function updateImageSize(value: string) {
    setImageSize(normalizeImageSize(value));
  }

  function chooseImageSize(value: string) {
    updateImageSize(value);
    closeParamPickers();
  }

  function chooseImageModel(model: string) {
    setSelectedImageModel(model);
    closeParamPickers();
  }

  function chooseImageCount(count: number) {
    setImageCount(count);
    closeParamPickers();
  }

  function toggleModelPicker() {
    setIsModelPickerOpen((open) => !open);
    setIsCountPickerOpen(false);
    setIsSizePickerOpen(false);
  }

  function toggleCountPicker() {
    setIsCountPickerOpen((open) => !open);
    setIsModelPickerOpen(false);
    setIsSizePickerOpen(false);
  }

  function toggleSizePicker() {
    setIsSizePickerOpen((open) => !open);
    setIsModelPickerOpen(false);
    setIsCountPickerOpen(false);
  }

  function updateSelectedModelRecords(modelId: string) {
    setSelectedImageModels((current) => {
      const orderedIds = modelIds(newestFirstModels(fetchedImageModels));
      if (current.includes(modelId)) {
        if (current.length === 1) return current;
        const next = orderedIds.filter((id) => current.includes(id) && id !== modelId);
        if (selectedImageModel === modelId) {
          setSelectedImageModel(next[0] ?? '');
        }
        return next;
      }

      const nextSelected = new Set([...current, modelId]);
      return orderedIds.filter((id) => nextSelected.has(id));
    });
  }

  async function fetchProviderModels() {
    setIsFetchingModels(true);
    setStatus('正在获取图片模型列表...');
    setSettingsStatus('正在获取图片模型列表...');
    try {
      const models = await invoke<ProviderModel[]>('fetch_provider_models', {
        input: { ...providerForm, image_model: null },
      });
      const fetchedModelIds = models.map((model) => model.id);
      const orderedModelIds = [...fetchedModelIds].reverse();
      setFetchedImageModels(models);
      if (fetchedModelIds.length === 0) {
        setSelectedImageModels([]);
        setSelectedImageModel('');
        setStatus('未从模型列表中识别到图片模型');
        setSettingsStatus('接口可访问，但没有识别到图片模型');
        return;
      }

      setSelectedImageModels((current) => {
        const kept = orderedModelIds.filter((model) => current.includes(model));
        const next = [...kept, ...orderedModelIds.filter((model) => !kept.includes(model))];
        setSelectedImageModel((selected) => selectedOrDefaultModel(next, selected));
        return next;
      });
      setStatus(`已获取 ${fetchedModelIds.length} 个图片模型`);
      setSettingsStatus(`已获取 ${fetchedModelIds.length} 个图片模型`);
    } catch (error) {
      setStatus(`获取模型失败：${formatError(error)}`);
      setSettingsStatus(`获取模型失败：${formatError(error)}`);
    } finally {
      setIsFetchingModels(false);
    }
  }

  async function refreshProviders() {
    const result = await invoke<ProviderConfig[]>('list_providers');
    setProviders(result);
    const current = result.find((provider) => provider.id === providerForm.id) ?? result[0];
    if (current) {
      const capabilities = parseProviderCapabilities(current.capabilities);
      const storedModels = capabilities.image_models ?? [];
      const orderedModelIds = modelIds(newestFirstModels(storedModels));
      const storedSelectedModels = selectedModelIdsInOrder(
        orderedModelIds,
        capabilities.selected_image_models,
      );
      const nextSelectedModels =
        storedSelectedModels.length > 0
          ? storedSelectedModels
          : storedModels.length > 0
            ? orderedModelIds
            : defaultImageModelOptions;
      setFetchedImageModels(storedModels);
      setSelectedImageModels(nextSelectedModels);
      setSelectedImageModel((selected) => selectedOrDefaultModel(nextSelectedModels, selected));
      setProviderForm((form) => ({
        ...form,
        id: current.id,
        name: current.name,
        kind: current.kind,
        base_url: current.base_url,
        api_key: current.api_key ?? '',
        text_model: current.text_model ?? null,
        image_model: null,
        enabled: current.enabled,
      }));
    }
    return result;
  }

  async function saveProvider() {
    if (!providerForm.api_key.trim()) {
      setStatus('请先填写 API Key，再保存配置');
      setSettingsStatus('请先填写 API Key，再保存配置');
      return;
    }

    setIsBusy(true);
    setStatus('正在保存配置...');
    setSettingsStatus('正在保存配置...');
    try {
      await invoke('upsert_provider', {
        input: {
          ...providerForm,
          image_model: null,
          capabilities: buildProviderCapabilities(fetchedImageModels, selectedImageModels),
        },
      });
      await refreshProviders();
      setStatus('配置已保存');
      setSettingsStatus('配置已保存');
    } catch (error) {
      setStatus(`保存失败：${formatError(error)}`);
      setSettingsStatus(`保存失败：${formatError(error)}`);
    } finally {
      setIsBusy(false);
    }
  }

  async function deleteProvider(id: string) {
    setIsBusy(true);
    setStatus('正在删除配置...');
    setSettingsStatus('正在删除配置...');
    try {
      const wasSelected = providerForm.id === id;
      await invoke('delete_provider', { id });
      const remainingProviders = await refreshProviders();
      if (wasSelected && remainingProviders.length === 0) {
        setProviderForm(defaultProviderForm);
        setFetchedImageModels([]);
        setSelectedImageModels([]);
        setSelectedImageModel('');
      }
      setStatus('配置已删除');
      setSettingsStatus('配置已删除');
    } catch (error) {
      setStatus(`删除失败：${formatError(error)}`);
      setSettingsStatus(`删除失败：${formatError(error)}`);
    } finally {
      setIsBusy(false);
    }
  }

  function loadProvider(provider: ProviderConfig) {
    closeParamPickers();
    setProviderForm((form) => ({
      ...form,
      id: provider.id,
      name: provider.name,
      kind: provider.kind,
      base_url: provider.base_url,
      api_key: provider.api_key ?? '',
      text_model: provider.text_model ?? null,
      image_model: null,
      enabled: provider.enabled,
    }));
    const capabilities = parseProviderCapabilities(provider.capabilities);
    const storedModels = capabilities.image_models ?? [];
    const orderedModelIds = modelIds(newestFirstModels(storedModels));
    const storedSelectedModels = selectedModelIdsInOrder(
      orderedModelIds,
      capabilities.selected_image_models,
    );
    const nextSelectedModels =
      storedSelectedModels.length > 0
        ? storedSelectedModels
        : storedModels.length > 0
          ? orderedModelIds
          : defaultImageModelOptions;
    setFetchedImageModels(storedModels);
    setSelectedImageModels(nextSelectedModels);
    setSelectedImageModel(nextSelectedModels[0] ?? '');
    setStatus('已切换模型配置');
    setSettingsStatus('');
  }

  async function generateImage() {
    if (!selectedImageModel) {
      setStatus('请先在设置中获取并选择图像模型');
      return;
    }
    if (!providerForm.api_key.trim()) {
      setStatus('请先在设置中填写并保存 API Key');
      return;
    }
    if (!providers.some((provider) => provider.id === providerForm.id)) {
      setStatus('请先保存当前模型供应商配置，再开始生成');
      return;
    }
    const generationMaterialPaths = materialPaths;
    const invalidReferenceIndex = invalidReferenceMentionIndex(prompt, generationMaterialPaths.length);
    if (invalidReferenceIndex) {
      setStatus(`提示词引用了 @参考图${invalidReferenceIndex}，但当前只有 ${generationMaterialPaths.length} 张参考图`);
      return;
    }
    setIsBusy(true);
    closeParamPickers();
    const totalCount = imageCount;
    const providerId = providerForm.id;
    const displayPrompt = prompt;
    const generationPrompt = buildPromptForReferenceImages(displayPrompt, generationMaterialPaths.length);
    const generationModel = selectedImageModel;
    const generationSize = imageSize;
    const requestIds = Array.from({ length: totalCount }, () => createRequestId());
    setActiveGenerationRequestIds(requestIds);
    setGenerationSteps(initialGenerationSteps);
    setStatus(`正在生成 ${totalCount} 张图片...`);
    try {
      startStep(0);
      await new Promise((resolve) => window.setTimeout(resolve, 80));
      startStep(1);
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      startStep(2);

      let saveStepStarted = false;
      let completedCount = 0;
      let successCount = 0;
      let failedCount = 0;
      const failedMessages: string[] = [];
      const completedPaths: string[] = [];
      const progressStatus = () => {
        const failedText = failedCount > 0 ? `，失败 ${failedCount} 张` : '';
        return `正在生成 ${totalCount} 张，已完成 ${completedCount}/${totalCount}${failedText}`;
      };

      await Promise.all(
        requestIds.map(async (requestId) => {
          try {
            const result = await invoke<GenerateImageOutput>('generate_image', {
              input: {
                provider_id: providerId,
                request_id: requestId,
                prompt: generationPrompt,
                model: generationModel,
                size: generationSize === 'auto' ? null : generationSize,
                image_paths: generationMaterialPaths,
              },
            });

            if (!saveStepStarted) {
              saveStepStarted = true;
              startStep(3);
            }
            completedCount += 1;
            successCount += 1;
            completedPaths.push(result.asset.file_path);
            const createdAt = new Date().toLocaleString();
            setSessionImages((images) => [
              {
                id: result.asset.id,
                file_path: result.asset.file_path,
                display_path: result.asset.display_path ?? null,
                prompt: displayPrompt,
                created_at: createdAt,
              },
              ...images,
            ]);
            setStatus(progressStatus());
          } catch (error) {
            completedCount += 1;
            failedCount += 1;
            failedMessages.push(formatError(error));
            setStatus(progressStatus());
          } finally {
            setActiveGenerationRequestIds((ids) => ids.filter((id) => id !== requestId));
          }
        }),
      );

      if (successCount > 0) {
        startStep(4);
        await refreshProviders();
        setStep(4, 'done');
      } else {
        setGenerationSteps((steps) =>
          steps.map((step) => (step.status === 'active' ? { ...step, status: 'error' } : step)),
        );
      }

      if (failedCount === 0) {
        setStatus(
          successCount === 1
            ? `生成完成：${completedPaths[0]}`
            : `生成完成：${successCount}/${totalCount} 张`,
        );
      } else if (successCount > 0) {
        setStatus(`生成完成 ${successCount}/${totalCount} 张，失败 ${failedCount} 张：${failedMessages[0]}`);
      } else {
        const message = failedMessages[0] ?? '未知错误';
        setStatus(message.includes('生成已强制停止') ? '已强制停止生成' : `生成失败：${message}`);
      }
    } catch (error) {
      setGenerationSteps((steps) =>
        steps.map((step) => (step.status === 'active' ? { ...step, status: 'error' } : step)),
      );
      const message = formatError(error);
      setStatus(message.includes('生成已强制停止') ? '已强制停止生成' : `生成失败：${message}`);
    } finally {
      setIsBusy(false);
      setActiveGenerationRequestIds([]);
    }
  }

  async function stopGeneration() {
    if (activeGenerationRequestIds.length === 0) return;
    setStatus('正在强制停止生成...');
    try {
      await Promise.all(
        activeGenerationRequestIds.map((requestId) => invoke('cancel_generation', { requestId })),
      );
    } catch (error) {
      setStatus(`停止失败：${formatError(error)}`);
    }
  }

  async function pickMaterialImages() {
    setStatus('正在打开素材选择器...');
    try {
      const paths = await invoke<string[]>('pick_material_images');
      if (paths.length === 0) {
        setStatus('未选择素材');
        return;
      }
      await addMaterialImages(paths);
    } catch (error) {
      setStatus(`打开素材选择器失败：${formatError(error)}`);
    }
  }

  async function addMaterialImages(paths: string[]) {
    const imagePaths = Array.from(new Set(paths.filter(isMaterialImagePath)));
    if (imagePaths.length === 0) {
      setStatus('请导入 PNG/JPG/WEBP 图片');
      return;
    }

    setStatus('正在导入参考图...');
    let importedPaths: string[];
    try {
      importedPaths = await invoke<string[]>('import_material_images', { paths: imagePaths });
    } catch (error) {
      setStatus(`导入参考图失败：${formatError(error)}`);
      return;
    }

    const existingPaths = new Set(materialPaths);
    const nextPaths = importedPaths.filter((path) => !existingPaths.has(path));
    if (nextPaths.length === 0) {
      setStatus('这些参考图已导入');
      return;
    }

    setMaterialPaths((current) => Array.from(new Set([...current, ...nextPaths])));
    setStatus(`已导入 ${nextPaths.length} 张参考图`);
  }

  function cleanupMaterialImages(paths: string[]) {
    if (paths.length === 0 || isBusy) return;
    invoke('remove_material_images', { paths }).catch(() => undefined);
  }

  function handleMaterialDragOver(event: React.DragEvent<HTMLDivElement>) {
    if (isBusy) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
    materialDropHoverRef.current = true;
    setIsMaterialDropActive(true);
  }

  function handleMaterialDragEnter(event: React.DragEvent<HTMLDivElement>) {
    if (isBusy) return;
    event.preventDefault();
    materialDropHoverRef.current = true;
    setIsMaterialDropActive(true);
  }

  function handleMaterialDragLeave(event: React.DragEvent<HTMLDivElement>) {
    if (
      event.relatedTarget instanceof Node
      && event.currentTarget.contains(event.relatedTarget)
    ) {
      return;
    }
    materialDropHoverRef.current = false;
    setIsMaterialDropActive(false);
  }

  function handleMaterialDrop(event: React.DragEvent<HTMLDivElement>) {
    if (isBusy) return;
    event.preventDefault();
    materialDropHoverRef.current = false;
    setIsMaterialDropActive(false);
    const paths = droppedPathsFromDataTransfer(event.dataTransfer);
    if (paths.length === 0) {
      return;
    }
    void addMaterialImages(paths);
  }

  function removeMaterialImage(path: string) {
    const removedIndex = materialPaths.indexOf(path) + 1;
    setMaterialPaths((current) => current.filter((item) => item !== path));
    cleanupMaterialImages([path]);
    if (removedIndex > 0) {
      setPrompt((current) => rewriteReferenceMentionsAfterRemovedIndexes(current, [removedIndex]));
      setReferenceMentionRange(null);
    }
  }

  function clearMaterialImages() {
    const removedIndexes = materialPaths.map((_path, index) => index + 1);
    cleanupMaterialImages(materialPaths);
    setMaterialPaths([]);
    setPrompt((current) => rewriteReferenceMentionsAfterRemovedIndexes(current, removedIndexes));
    setReferenceMentionRange(null);
  }

  function moveMaterialImage(sourcePath: string, targetPath: string) {
    setMaterialPaths((current) => {
      const sourceIndex = current.indexOf(sourcePath);
      const targetIndex = current.indexOf(targetPath);
      if (sourceIndex === -1 || targetIndex === -1 || sourceIndex === targetIndex) return current;
      const next = [...current];
      const [movedPath] = next.splice(sourceIndex, 1);
      next.splice(targetIndex, 0, movedPath);
      return next;
    });
  }

  function setMaterialCardRef(path: string, element: HTMLElement | null) {
    if (element) {
      materialCardRefs.current.set(path, element);
    } else {
      materialCardRefs.current.delete(path);
    }
  }

  function startMaterialDrag(event: React.PointerEvent<HTMLElement>, path: string) {
    if (isBusy || event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest('button')) return;
    event.preventDefault();
    materialDragOriginRef.current = { x: event.clientX, y: event.clientY };
    setDraggedMaterialPath(path);
    setDragOverMaterialPath(null);
    setMaterialDragOffset({ x: 0, y: 0 });
  }

  function endMaterialDrag() {
    materialDragOriginRef.current = null;
    setDraggedMaterialPath(null);
    setDragOverMaterialPath(null);
    setMaterialDragOffset({ x: 0, y: 0 });
  }

  async function revealImage(path: string) {
    try {
      await invoke('reveal_path', { path });
    } catch (error) {
      setStatus(`打开文件位置失败：${formatError(error)}`);
    }
  }

  async function openGeneratedDir() {
    try {
      await invoke('open_generated_dir');
    } catch (error) {
      setStatus(`打开保存目录失败：${formatError(error)}`);
    }
  }

  async function refreshGalleryDirectory() {
    const result = await invoke<GalleryDirectoryInfo>('get_gallery_directory');
    setGalleryInfo(result);
    return result;
  }

  async function openGalleryManager() {
    setIsGalleryOpen(true);
    setGalleryStatus('');
    try {
      await refreshGalleryDirectory();
    } catch (error) {
      setGalleryStatus(`读取图库目录失败：${formatError(error)}`);
    }
  }

  async function chooseGalleryDirectory() {
    setIsUpdatingGalleryDirectory(true);
    setGalleryStatus('请选择新的图库目录...');
    try {
      const directory = await invoke<string | null>('pick_gallery_directory');
      if (!directory) {
        setGalleryStatus('未选择目录');
        return;
      }
      const result = await invoke<SetGalleryDirectoryOutput>('set_gallery_directory', { directory });
      setGalleryInfo(result.directory);
      if (result.moved_paths.length > 0) {
        const movedPathMap = new Map(result.moved_paths.map((path) => [path.old_path, path]));
        setSessionImages((images) =>
          images.map((image) => {
            const movedPath = movedPathMap.get(image.file_path);
            if (!movedPath) return image;
            return {
              ...image,
              file_path: movedPath.new_path,
              display_path: movedPath.display_path ?? null,
            };
          }),
        );
      }
      setGalleryStatus(
        result.moved_paths.length > 0
          ? `图库目录已更新，已迁移 ${result.moved_paths.length} 张图片`
          : '图库目录已更新',
      );
    } catch (error) {
      setGalleryStatus(`设置图库目录失败：${formatError(error)}`);
    } finally {
      setIsUpdatingGalleryDirectory(false);
    }
  }

  async function openCurrentGalleryDirectory() {
    try {
      await invoke('open_generated_dir');
      if (!galleryInfo) {
        await refreshGalleryDirectory();
      }
      setGalleryStatus('已打开当前目录');
    } catch (error) {
      setGalleryStatus(`打开当前目录失败：${formatError(error)}`);
    }
  }

  async function checkForUpdates(options?: { silent?: boolean; autoOpen?: boolean }) {
    setIsCheckingUpdate(true);
    if (!options?.silent) {
      setUpdateStatus('正在检查 GitHub Releases...');
    }
    try {
      const result = await invoke<UpdateInfo>('check_for_updates');
      setUpdateInfo(result);
      if (result.has_update) {
        setUpdateStatus(`发现新版本 ${result.latest_tag}`);
        if (options?.autoOpen && !autoUpdateDismissedRef.current) {
          setIsUpdateOpen(true);
        }
      } else if (!options?.silent) {
        setUpdateStatus(`当前已是最新版本 ${result.current_version}`);
      } else {
        setUpdateStatus('');
      }
    } catch (error) {
      if (!options?.silent) {
        setUpdateStatus(`检查更新失败：${formatError(error)}`);
      }
    } finally {
      setIsCheckingUpdate(false);
    }
  }

  function openUpdateManager() {
    setIsUpdateOpen(true);
    if (!updateInfo && !isCheckingUpdate) {
      checkForUpdates().catch(() => undefined);
    }
  }

  function closeUpdateManager() {
    autoUpdateDismissedRef.current = true;
    setIsUpdateOpen(false);
  }

  async function openUpdateUrl(url: string) {
    try {
      await invoke('open_update_url', { url });
    } catch (error) {
      setUpdateStatus(`打开更新地址失败：${formatError(error)}`);
    }
  }

  async function downloadUpdateAsset() {
    if (!updateInfo?.asset) return;

    setIsDownloadingUpdate(true);
    setIsUpdateDownloadPaused(false);
    if (updateDownloadProgress?.file_name !== updateInfo.asset.name) {
      setUpdateDownloadProgress({
        downloaded_bytes: 0,
        file_name: updateInfo.asset.name,
        total_bytes: null,
      });
    }
    setUpdateStatus(updateDownloadProgress?.downloaded_bytes ? '正在继续下载...' : '正在下载安装包...');
    try {
      const result = await invoke<UpdateDownloadInfo>('download_update_asset', {
        url: updateInfo.asset.download_url,
        fileName: updateInfo.asset.name,
      });
      setUpdateDownloadProgress((current) =>
        current
          ? {
              ...current,
              downloaded_bytes: current.total_bytes ?? current.downloaded_bytes,
            }
          : current,
      );
      setUpdateStatus(`已下载并打开安装包：${result.file_path}`);
    } catch (error) {
      const message = formatError(error);
      if (message.includes('下载已暂停')) {
        setIsUpdateDownloadPaused(true);
        setUpdateStatus('下载已暂停，可继续下载');
      } else if (message.includes('下载已取消')) {
        setIsUpdateDownloadPaused(false);
        setUpdateDownloadProgress(null);
        setUpdateStatus('已关闭下载');
      } else {
        setIsUpdateDownloadPaused(false);
        setUpdateStatus(`下载更新失败：${message}`);
        setUpdateDownloadProgress(null);
      }
    } finally {
      setIsDownloadingUpdate(false);
    }
  }

  async function pauseUpdateDownload() {
    if (!updateInfo?.asset) return;
    setUpdateStatus('正在暂停下载...');
    try {
      const didPause = await invoke<boolean>('pause_update_download', { fileName: updateInfo.asset.name });
      if (!didPause) {
        setIsDownloadingUpdate(false);
        setIsUpdateDownloadPaused(Boolean(updateDownloadProgress));
        setUpdateStatus(updateDownloadProgress ? '下载已暂停，可继续下载' : '没有正在下载的安装包');
      }
    } catch (error) {
      setUpdateStatus(`暂停下载失败：${formatError(error)}`);
    }
  }

  async function cancelUpdateDownload() {
    if (!updateInfo?.asset && !updateDownloadProgress?.file_name) return;
    const fileName = updateInfo?.asset?.name ?? updateDownloadProgress?.file_name;
    if (!fileName) return;

    setUpdateStatus('正在关闭下载...');
    try {
      await invoke('cancel_update_download', { fileName });
      if (!isDownloadingUpdate) {
        setIsUpdateDownloadPaused(false);
        setUpdateDownloadProgress(null);
        setUpdateStatus('已关闭下载');
      }
    } catch (error) {
      setUpdateStatus(`关闭下载失败：${formatError(error)}`);
    }
  }

  useEffect(() => {
    invoke('clear_material_image_cache').catch(() => undefined);
    invoke('clear_generated_image_preview_cache').catch(() => undefined);
    refreshProviders().catch(() => setStatus('后端未启动或数据库初始化失败'));
    checkForUpdates({ silent: true, autoOpen: true }).catch(() => undefined);
    closeParamPickers();
  }, []);

  useEffect(() => {
    let isDisposed = false;
    let unlisten: (() => void) | undefined;

    listen<UpdateDownloadProgress>('update-download-progress', (event) => {
      setUpdateDownloadProgress(event.payload);
    })
      .then((nextUnlisten) => {
        if (isDisposed) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch(() => undefined);

    return () => {
      isDisposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    setActiveReferenceMentionOptionIndex(0);
  }, [referenceMentionRange?.query, referenceMentionRange?.start]);

  useEffect(() => {
    setActiveReferenceMentionOptionIndex((current) =>
      Math.min(current, Math.max(referenceMentionOptions.length - 1, 0)),
    );
  }, [referenceMentionOptions.length]);

  useLayoutEffect(() => {
    if (!referenceMentionRange) {
      setReferenceMentionPopoverPosition(null);
      return undefined;
    }

    function updateReferenceMentionPopoverPosition() {
      const inputWrap = promptInputWrapRef.current;
      if (!inputWrap) return;

      const rect = inputWrap.getBoundingClientRect();
      const viewportPadding = 12;
      const availableWidth = Math.max(120, window.innerWidth - viewportPadding * 2);
      const preferredWidth = Math.min(260, Math.max(180, rect.width - 24));
      const width = Math.min(preferredWidth, availableWidth);
      const maxLeft = Math.max(viewportPadding, window.innerWidth - width - viewportPadding);
      const left = Math.min(Math.max(viewportPadding, rect.left + 12), maxLeft);
      const preferredTop = rect.top + 38;
      const availableBelow = window.innerHeight - preferredTop - viewportPadding;
      const maxHeight = Math.min(236, Math.max(96, availableBelow));
      const top = availableBelow >= 96
        ? preferredTop
        : Math.max(viewportPadding, window.innerHeight - maxHeight - viewportPadding);
      setReferenceMentionPopoverPosition({ left, maxHeight, top, width });
    }

    updateReferenceMentionPopoverPosition();
    window.addEventListener('resize', updateReferenceMentionPopoverPosition);
    window.addEventListener('scroll', updateReferenceMentionPopoverPosition, true);
    return () => {
      window.removeEventListener('resize', updateReferenceMentionPopoverPosition);
      window.removeEventListener('scroll', updateReferenceMentionPopoverPosition, true);
    };
  }, [referenceMentionRange, materialPaths.length]);

  useEffect(() => {
    if (!isModelPickerOpen && !isCountPickerOpen && !isSizePickerOpen) return undefined;

    function closeParamPopovers(event: PointerEvent) {
      const target = event.target;
      if (
        target instanceof Element
        && paramsRef.current?.contains(target)
        && (
          target.closest('.model-popover-host')
          || target.closest('.count-popover-host')
          || target.closest('.size-popover-host')
        )
      ) {
        return;
      }
      setIsModelPickerOpen(false);
      setIsCountPickerOpen(false);
      setIsSizePickerOpen(false);
    }

    window.addEventListener('pointerdown', closeParamPopovers);
    return () => window.removeEventListener('pointerdown', closeParamPopovers);
  }, [isModelPickerOpen, isCountPickerOpen, isSizePickerOpen]);

  useEffect(() => {
    if (!isTauriRuntime()) return undefined;

    let isDisposed = false;
    let unlisten: (() => void) | undefined;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (isBusy) {
          setIsMaterialDropActive(false);
          return;
        }

        if (event.payload.type === 'leave') {
          materialDropHoverRef.current = false;
          setIsMaterialDropActive(false);
          return;
        }

        const point = clientPointFromDragPosition(event.payload.position);
        const isInsideDropZone = isPointInsideElement(materialDropZoneRef.current, point.x, point.y);
        if (event.payload.type === 'drop') {
          materialDropHoverRef.current = false;
          setIsMaterialDropActive(false);
          if (event.payload.paths.some(isMaterialImagePath)) {
            void addMaterialImages(event.payload.paths);
          }
          return;
        }

        const isImageDrag =
          event.payload.type === 'enter' && event.payload.paths.some(isMaterialImagePath);
        setIsMaterialDropActive(isImageDrag || isInsideDropZone || materialDropHoverRef.current);
      })
      .then((nextUnlisten) => {
        if (isDisposed) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch(() => undefined);

    return () => {
      isDisposed = true;
      unlisten?.();
    };
  }, [isBusy, materialPaths]);

  useEffect(() => {
    if (!draggedMaterialPath) return undefined;
    const activeDraggedMaterialPath = draggedMaterialPath;

    function moveMaterialDrag(event: PointerEvent) {
      const origin = materialDragOriginRef.current;
      if (!origin) return;

      setMaterialDragOffset({
        x: event.clientX - origin.x,
        y: event.clientY - origin.y,
      });

      const sourcePath = activeDraggedMaterialPath;
      const sourceIndex = materialPaths.indexOf(sourcePath);
      if (sourceIndex === -1) return;

      let nextTargetPath: string | null = null;
      for (const targetPath of materialPaths) {
        if (targetPath === sourcePath) continue;

        const targetElement = materialCardRefs.current.get(targetPath);
        if (!targetElement) continue;

        const targetRect = targetElement.getBoundingClientRect();
        const isInsideTarget =
          event.clientX >= targetRect.left
          && event.clientX <= targetRect.right
          && event.clientY >= targetRect.top
          && event.clientY <= targetRect.bottom;
        if (!isInsideTarget) continue;

        const targetIndex = materialPaths.indexOf(targetPath);
        const targetMiddleX = targetRect.left + targetRect.width / 2;
        const targetMiddleY = targetRect.top + targetRect.height / 2;
        const sourceRow = Math.floor(sourceIndex / 3);
        const targetRow = Math.floor(targetIndex / 3);
        const crossedMiddle =
          sourceRow === targetRow
            ? sourceIndex < targetIndex
              ? event.clientX > targetMiddleX
              : event.clientX < targetMiddleX
            : sourceIndex < targetIndex
              ? event.clientY > targetMiddleY
              : event.clientY < targetMiddleY;

        nextTargetPath = targetPath;
        if (crossedMiddle) {
          moveMaterialImage(sourcePath, targetPath);
          materialDragOriginRef.current = { x: event.clientX, y: event.clientY };
          setMaterialDragOffset({ x: 0, y: 0 });
        }
        break;
      }

      setDragOverMaterialPath(nextTargetPath);
    }

    window.addEventListener('pointermove', moveMaterialDrag);
    window.addEventListener('pointerup', endMaterialDrag);
    window.addEventListener('pointercancel', endMaterialDrag);
    return () => {
      window.removeEventListener('pointermove', moveMaterialDrag);
      window.removeEventListener('pointerup', endMaterialDrag);
      window.removeEventListener('pointercancel', endMaterialDrag);
    };
  }, [draggedMaterialPath, materialPaths]);

  const referenceMentionPopover = referenceMentionRange && referenceMentionPopoverPosition
    ? createPortal(
        <div
          className="reference-mention-popover"
          role="listbox"
          style={{
            left: referenceMentionPopoverPosition.left,
            maxHeight: referenceMentionPopoverPosition.maxHeight,
            top: referenceMentionPopoverPosition.top,
            width: referenceMentionPopoverPosition.width,
          }}
        >
          {materialPaths.length === 0 ? (
            <div className="reference-mention-empty">先上传参考图</div>
          ) : referenceMentionOptions.length === 0 ? (
            <div className="reference-mention-empty">没有匹配的参考图</div>
          ) : (
            referenceMentionOptions.map((item, optionIndex) => (
              <button
                aria-selected={optionIndex === activeReferenceMentionOptionIndex}
                className={`reference-mention-item ${optionIndex === activeReferenceMentionOptionIndex ? 'active' : ''}`}
                key={item.path}
                onMouseEnter={() => setActiveReferenceMentionOptionIndex(optionIndex)}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => chooseReferenceMention(item.index)}
                role="option"
                type="button"
              >
                <img src={convertFileSrc(item.path)} alt="" />
                <span>参考图{item.index}</span>
              </button>
            ))
          )}
        </div>,
        document.body,
      )
    : null;

  return (
    <main className="app-shell">
      {referenceMentionPopover}
      <aside className="side-rail">
        <div className="rail-logo">
          <img src={appLogo} alt="Image Draw AI" />
        </div>
        <nav className="rail-nav" aria-label="主导航">
          <button className="rail-button active" title="生成">
            <span className="rail-icon"><RobotOutlined /></span>
            <strong>生成</strong>
          </button>
          <button className="rail-button" title="图库" onClick={openGalleryManager}>
            <span className="rail-icon"><FolderOpenOutlined /></span>
            <strong>图库</strong>
          </button>
        </nav>
        <div className="rail-bottom">
          <button
            className={`rail-button ${updateInfo?.has_update ? 'has-update' : ''}`}
            title="软件更新"
            onClick={openUpdateManager}
          >
            <span className="rail-icon"><VerticalAlignTopOutlined /></span>
            <strong>更新</strong>
          </button>
          <button className="rail-button rail-settings" title="设置" onClick={() => setIsSettingsOpen(true)}>
            <span className="rail-icon"><SettingOutlined /></span>
            <strong>设置</strong>
          </button>
        </div>
      </aside>

      <section className="app-stage">
        <header className="topbar">
          <div className="brand">
            <div>
              <p>Image Draw AI</p>
              <h1>图像生成工作台</h1>
            </div>
          </div>
          <div className="topbar-actions">
            <div className="current-provider">
              <span>供应商</span>
              <strong>{activeProviderName}</strong>
            </div>
            <button className="ghost" onClick={() => setIsSettingsOpen(true)}>设置</button>
          </div>
        </header>

        <section className="workspace">
          <aside className="compose-card">
            <div className="section-heading">
              <div>
                <span>创作区</span>
                <strong>{activeMode}</strong>
              </div>
              <small>{imageAspectRatio} / {imageSize} / {imageCount} 张</small>
            </div>

            <div className="field prompt-field">
              <span>提示词</span>
              <div className="prompt-input-wrap" ref={promptInputWrapRef}>
                <div
                  aria-hidden="true"
                  className="prompt-highlight"
                  style={{ transform: `translateY(-${promptScrollTop}px)` }}
                >
                  {promptHighlightParts(prompt, materialPaths.length).map((part, index) => (
                    <span
                      className={part.isReferenceMention ? 'prompt-linked-reference' : undefined}
                      key={`${index}-${part.text}`}
                    >
                      {part.text}
                    </span>
                  ))}
                </div>
                <textarea
                  ref={promptTextareaRef}
                  disabled={isBusy}
                  value={prompt}
                  onBlur={() => window.setTimeout(() => setReferenceMentionRange(null), 120)}
                  onChange={(event) => updatePrompt(event.target.value, event.target.selectionStart)}
                  onClick={refreshReferenceMention}
                  onKeyDown={handlePromptKeyDown}
                  onKeyUp={refreshReferenceMention}
                  onScroll={(event) => setPromptScrollTop(event.currentTarget.scrollTop)}
                />
              </div>
            </div>

            <div className="material-panel">
              <div className="material-header">
                <div>
                  <strong>参考图</strong>
                  <span>{materialPaths.length > 0 ? `${materialPaths.length} 张素材` : '未导入'}</span>
                </div>
                <div className="material-actions">
                  <button className="ghost mini" onClick={pickMaterialImages} disabled={isBusy}>上传</button>
                  {materialPaths.length > 0 && (
                    <button className="ghost mini" onClick={clearMaterialImages} disabled={isBusy}>清空</button>
                  )}
                </div>
              </div>

              <div
                className={`reference-grid ${materialPaths.length === 0 ? 'empty' : ''} ${isMaterialDropActive ? 'drag-active' : ''}`}
                onDragEnter={handleMaterialDragEnter}
                onDragLeave={handleMaterialDragLeave}
                onDragOver={handleMaterialDragOver}
                onDrop={handleMaterialDrop}
                ref={materialDropZoneRef}
              >
                {materialPaths.length === 0 && (
                  <div className="reference-empty">
                    {isMaterialDropActive ? '松开导入参考图' : '未导入参考图'}
                  </div>
                )}
                {materialPaths.map((path, index) => (
                  <article
                    className={`reference-card ${isBusy ? 'disabled' : ''} ${draggedMaterialPath === path ? 'dragging' : ''} ${dragOverMaterialPath === path ? 'drag-over' : ''}`}
                    key={path}
                    onPointerDown={(event) => startMaterialDrag(event, path)}
                    onPointerUp={endMaterialDrag}
                    ref={(element) => setMaterialCardRef(path, element)}
                    style={
                      draggedMaterialPath === path
                        ? ({
                            '--drag-x': `${materialDragOffset.x}px`,
                            '--drag-y': `${materialDragOffset.y}px`,
                          } as React.CSSProperties)
                        : undefined
                    }
                    title={isBusy ? undefined : '拖动调整顺序'}
                  >
                    <img src={convertFileSrc(path)} alt="素材图片" />
                    <span>{index + 1}</span>
                    <div className="reference-card-actions">
                      <button
                        aria-label="移除参考图"
                        className="reference-card-action reference-card-remove"
                        disabled={isBusy}
                        onClick={() => removeMaterialImage(path)}
                        title="移除"
                        type="button"
                      >
                        ×
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            </div>

            <div className="params-card" ref={paramsRef}>
              <div className="param-toolbar">
                <div className="param-popover-host model-popover-host">
                  <button
                    aria-expanded={isModelPickerOpen}
                    className={`param-trigger icon-only ${isModelPickerOpen ? 'active' : ''}`}
                    disabled={isBusy}
                    onClick={toggleModelPicker}
                    title={selectedImageModel || '选择图像模型'}
                    type="button"
                  >
                    <RobotOutlined />
                    <span>{selectedImageModel || '模型'}</span>
                  </button>
                  {isModelPickerOpen && (
                    <div className="param-popover model-picker-popover">
                      <div className="model-picker-list">
                        {visibleImageModelOptions.length === 0 ? (
                          <p>未获取模型</p>
                        ) : (
                          visibleImageModelOptions.map((model) => (
                            <button
                              className={`model-picker-item ${selectedImageModel === model ? 'active' : ''}`}
                              key={model}
                              onClick={() => chooseImageModel(model)}
                              type="button"
                            >
                              <RobotOutlined />
                              <span>{model}</span>
                            </button>
                          ))
                        )}
                      </div>
                    </div>
                  )}
                </div>

                <div className="param-popover-host count-popover-host">
                  <button
                    aria-expanded={isCountPickerOpen}
                    className={`param-trigger count-trigger ${isCountPickerOpen ? 'active' : ''}`}
                    disabled={isBusy}
                    onClick={toggleCountPicker}
                    title={`图片数量：${imageCount}`}
                    type="button"
                  >
                    <PictureOutlined />
                    <span>{imageCount} 张</span>
                  </button>
                  {isCountPickerOpen && (
                    <div className="param-popover count-picker-popover">
                      <div className="param-popover-title">
                        <span>配置</span>
                        <strong>图片数量</strong>
                      </div>
                      <div className="choice-options count-options" role="group" aria-label="图片数量">
                        {imageCountOptions.map((count) => (
                          <button
                            aria-pressed={imageCount === count}
                            className={`choice-option ${imageCount === count ? 'active' : ''}`}
                            key={count}
                            onClick={() => chooseImageCount(count)}
                            type="button"
                          >
                            {count}
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>

                <div className="param-popover-host size-popover-host">
                  <button
                    aria-expanded={isSizePickerOpen}
                    className={`param-trigger ${isSizePickerOpen ? 'active' : ''}`}
                    disabled={isBusy}
                    onClick={toggleSizePicker}
                    title={`尺寸：${selectedSizeOption.label}`}
                    type="button"
                  >
                    <SettingOutlined />
                    <span>{selectedSizeOption.label}</span>
                  </button>
                  {isSizePickerOpen && (
                    <div className="param-popover size-picker-popover">
                      <div className="param-popover-title">
                        <span>配置</span>
                        <strong>尺寸</strong>
                      </div>
                      <div className="size-options" role="group" aria-label="尺寸">
                        {imageSizeOptions.map((size) => (
                          <button
                            aria-pressed={imageSize === size.value}
                            className={`size-option ${imageSize === size.value ? 'active' : ''}`}
                            key={size.value}
                            onClick={() => chooseImageSize(size.value)}
                            type="button"
                          >
                            <span className={`size-option-icon ${size.shape}`} aria-hidden="true" />
                            <span>{size.label}</span>
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>

            <div className="generation-actions">
              <button className="generate-button" onClick={generateImage} disabled={isBusy}>
                {isBusy ? '正在生成...' : '开始生成'}
              </button>
              <button
                aria-label="强制停止生成"
                className="stop-button"
                onClick={stopGeneration}
                disabled={!isBusy || activeGenerationRequestIds.length === 0}
                title="强制停止"
              >
                <StopOutlined />
              </button>
            </div>

            {status !== '准备就绪' && (
              <p className={`status ${isErrorStatus(status) ? 'error' : ''}`}>{status}</p>
            )}
          </aside>

          <section className="result-card">
            <div className="section-heading result-heading">
              <div>
                <span>结果区</span>
                <strong>本次生成 {sessionImages.length} 张</strong>
              </div>
              <div className="heading-actions">
                <button className="ghost mini" onClick={openGeneratedDir}>打开目录</button>
              </div>
            </div>

            {sessionImages.length === 0 ? (
              <div className="empty-state">
                <img src={appLogo} alt="" />
                <div>等待首张作品</div>
                <p>{selectedImageModel || '未获取模型'} / {imageSize} / {imageCount} 张</p>
              </div>
            ) : (
              <div className="image-grid">
                {sessionImages.map((image) => (
                  <article className="image-card" key={image.id}>
                    <button className="image-preview-button" onClick={() => setPreviewImage(image)}>
                      <img src={convertFileSrc(imageDisplayPath(image))} alt={image.prompt} />
                    </button>
                    <div>
                      <strong>{image.created_at}</strong>
                      <p>{image.prompt}</p>
                      <button className="ghost mini" onClick={() => revealImage(image.file_path)}>定位文件</button>
                      <span>{image.file_path}</span>
                    </div>
                  </article>
                ))}
              </div>
            )}

            <div className={`progress-card result-progress ${isBusy ? 'is-loading' : ''}`}>
              <div className="spinner" aria-hidden="true" />
              <div className="progress-content">
                <strong>{isBusy ? '生成中' : '生成流程'}</strong>
                <ol className="step-list">
                  {generationSteps.map((step) => (
                    <li className={`step ${step.status}`} key={step.label}>
                      <span />
                      {step.label}
                    </li>
                  ))}
                </ol>
              </div>
            </div>
          </section>
        </section>
      </section>

      {isSettingsOpen && (
        <div className="drawer-layer">
          <button className="drawer-mask" onClick={() => setIsSettingsOpen(false)} aria-label="关闭设置" />
          <aside className="settings-drawer">
            <div className="drawer-header">
              <div>
                <span>设置</span>
                <h2>模型供应商</h2>
              </div>
              <button className="ghost" onClick={() => setIsSettingsOpen(false)}>关闭</button>
            </div>

            <div className="settings-content">
              <div className="settings-form">
                <label className="field">
                  <span>配置 ID</span>
                  <input value={providerForm.id} onChange={(event) => updateProviderForm('id', event.target.value)} />
                </label>
                <label className="field">
                  <span>名称</span>
                  <input value={providerForm.name} onChange={(event) => updateProviderForm('name', event.target.value)} />
                </label>
                <label className="field">
                  <span>API 分类</span>
                  <select value={providerForm.kind} onChange={(event) => updateProviderKind(event.target.value)}>
                    {apiKindOptions.map((option) => (
                      <option key={option.value} value={option.value} disabled={!option.supported}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field">
                  <span>Base URL</span>
                  <input
                    value={providerForm.base_url}
                    onChange={(event) => updateProviderForm('base_url', event.target.value)}
                    placeholder="https://api.openai.com/v1"
                  />
                </label>
                <label className="field">
                  <span>API Key</span>
                  <input
                    value={providerForm.api_key}
                    onChange={(event) => updateProviderForm('api_key', event.target.value)}
                    placeholder={apiKeyPlaceholder(providerForm.kind)}
                    type="password"
                  />
                </label>
                <p className="settings-tip">{providerSettingsTip(providerForm.kind)}</p>
              </div>

              <div className="drawer-actions">
                <button onClick={saveProvider} disabled={isBusy}>保存配置</button>
                <button className="ghost" onClick={fetchProviderModels} disabled={isBusy || isFetchingModels}>
                  {isFetchingModels ? '获取中...' : '获取模型'}
                </button>
              </div>

              {settingsStatus && (
                <p className={`settings-status ${isErrorStatus(settingsStatus) ? 'error' : ''}`}>
                  {settingsStatus}
                </p>
              )}

              <div className="model-list-panel">
                <div className="section-heading">
                  <span>模型列表</span>
                  <strong>{fetchedImageModels.length} 个图片模型</strong>
                </div>
                {fetchedImageModels.length === 0 ? (
                  <p className="muted model-empty">暂无图片模型</p>
                ) : (
                  <ul className="model-list">
                    {newestFirstModels(fetchedImageModels).map((model) => (
                      <li key={model.id}>
                        <label className="model-record">
                          <input
                            checked={selectedImageModels.includes(model.id)}
                            disabled={isBusy}
                            onChange={() => updateSelectedModelRecords(model.id)}
                            type="checkbox"
                          />
                          <span>
                            <strong>{model.id}</strong>
                            {model.owned_by && <small>{model.owned_by}</small>}
                          </span>
                        </label>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              <div className="saved-providers">
                <div className="section-heading">
                  <div>
                    <span>已保存</span>
                    <strong>{providers.length} 个配置</strong>
                  </div>
                  <button className="ghost mini" onClick={refreshProviders} disabled={isBusy}>刷新</button>
                </div>
                {providers.length === 0 ? (
                  <p className="muted">暂无配置，保存后会出现在这里。</p>
                ) : (
                  <ul className="provider-list">
                    {providers.map((provider) => (
                      <li key={provider.id}>
                        <div className="provider-item">
                          <button className="link-button" onClick={() => loadProvider(provider)} disabled={isBusy}>
                            <strong>{provider.name}</strong>
                            <span>{provider.base_url}</span>
                          </button>
                          <button className="danger" onClick={() => deleteProvider(provider.id)} disabled={isBusy}>删除</button>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

            </div>
          </aside>
        </div>
      )}

      {isGalleryOpen && (
        <div className="preview-layer">
          <button className="drawer-mask" onClick={() => setIsGalleryOpen(false)} aria-label="关闭图库设置" />
          <section className="gallery-modal">
            <div className="preview-header">
              <div>
                <span>图库</span>
                <h2>存储目录</h2>
              </div>
              <button className="ghost" onClick={() => setIsGalleryOpen(false)}>关闭</button>
            </div>

            <div className="gallery-body">
              <div className="gallery-path-box">
                <span>{galleryInfo?.is_custom ? '自定义目录' : '默认目录'}</span>
                <strong>{galleryInfo?.directory || '正在读取目录...'}</strong>
              </div>

              {galleryStatus && (
                <p className={`settings-status ${isErrorStatus(galleryStatus) ? 'error' : ''}`}>
                  {galleryStatus}
                </p>
              )}

              <div className="gallery-actions">
                <button onClick={chooseGalleryDirectory} disabled={isUpdatingGalleryDirectory}>
                  {isUpdatingGalleryDirectory ? '处理中...' : '选择目录并迁移'}
                </button>
                <button className="ghost" onClick={openCurrentGalleryDirectory}>打开当前目录</button>
              </div>
            </div>
          </section>
        </div>
      )}

      {isUpdateOpen && (
        <div className="preview-layer">
          <button className="drawer-mask" onClick={closeUpdateManager} aria-label="关闭软件更新" />
          <section className="update-modal">
            <div className="preview-header">
              <div>
                <span>软件更新</span>
                <h2>{updateInfo?.has_update ? `发现 ${updateInfo.latest_tag}` : 'GitHub Releases'}</h2>
              </div>
              <button className="ghost" onClick={closeUpdateManager}>关闭</button>
            </div>

            <div className="update-modal-body">
              {updateInfo ? (
                <div className={`update-summary ${updateInfo.has_update ? 'has-update' : ''}`}>
                  <strong>{updateInfo.has_update ? `可更新到 ${updateInfo.latest_tag}` : '当前已是最新版本'}</strong>
                  <span>当前版本 {updateInfo.current_version}</span>
                  <span>{updateInfo.release_name}</span>
                  {updateInfo.asset && <small>{updateInfo.asset.name}</small>}
                </div>
              ) : (
                <div className="update-summary">
                  <strong>{isCheckingUpdate ? '正在检查更新' : '尚未检查更新'}</strong>
                  <span>更新来源为 GitHub Releases</span>
                </div>
              )}

              {updateStatus && (
                <p className={`settings-status ${isErrorStatus(updateStatus) ? 'error' : ''}`}>
                  {updateStatus}
                </p>
              )}

              {updateDownloadProgress && (
                <div className="update-download-progress">
                  <div className="update-progress-header">
                    <span>{updateDownloadProgress.file_name}</span>
                    <strong>{updateDownloadPercent === null ? '下载中' : `${updateDownloadPercent}%`}</strong>
                  </div>
                  <div className="update-progress-track">
                    <div
                      className={updateDownloadPercent === null ? 'indeterminate' : ''}
                      style={updateDownloadPercent === null ? undefined : { width: `${updateDownloadPercent}%` }}
                    />
                  </div>
                  <small>
                    {formatBytes(updateDownloadProgress.downloaded_bytes)}
                    {updateDownloadProgress.total_bytes
                      ? ` / ${formatBytes(updateDownloadProgress.total_bytes)}`
                      : ''}
                  </small>
                  {(isDownloadingUpdate || isUpdateDownloadPaused) && (
                    <div className="update-progress-actions">
                      {isDownloadingUpdate ? (
                        <button className="ghost mini" onClick={pauseUpdateDownload} type="button">
                          暂停
                        </button>
                      ) : (
                        <button className="ghost mini" onClick={downloadUpdateAsset} type="button">
                          继续
                        </button>
                      )}
                      <button className="danger mini" onClick={cancelUpdateDownload} type="button">
                        关闭下载
                      </button>
                    </div>
                  )}
                </div>
              )}

              <div className="update-actions">
                <button className="ghost" onClick={() => checkForUpdates()} disabled={isCheckingUpdate}>
                  <SyncOutlined spin={isCheckingUpdate} />
                  {isCheckingUpdate ? '检查中' : '检查更新'}
                </button>
                <button
                  className="ghost"
                  onClick={() => updateInfo && openUpdateUrl(updateInfo.release_url)}
                  disabled={!updateInfo}
                >
                  打开 Release
                </button>
                <button
                  onClick={downloadUpdateAsset}
                  disabled={!updateInfo?.asset || isDownloadingUpdate}
                >
                  <CloudDownloadOutlined spin={isDownloadingUpdate} />
                  {isDownloadingUpdate ? '下载中' : isUpdateDownloadPaused ? '继续下载' : '下载并安装'}
                </button>
              </div>
            </div>
          </section>
        </div>
      )}

      {previewImage && (
        <div className="preview-layer">
          <button className="drawer-mask" onClick={() => setPreviewImage(null)} aria-label="关闭预览" />
          <section className="preview-modal">
            <div className="preview-header">
              <div>
                <span>图片预览</span>
                <h2>{previewImage.created_at}</h2>
              </div>
              <button className="ghost" onClick={() => setPreviewImage(null)}>关闭</button>
            </div>
            <img src={convertFileSrc(imageDisplayPath(previewImage))} alt={previewImage.prompt} />
            <div className="preview-info">
              <p>{previewImage.prompt}</p>
              <button className="ghost" onClick={() => revealImage(previewImage.file_path)}>在文件夹中显示</button>
              <span>{previewImage.file_path}</span>
            </div>
          </section>
        </div>
      )}
    </main>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(<App />);
