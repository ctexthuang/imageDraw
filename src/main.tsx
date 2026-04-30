import React, { useEffect, useRef, useState } from 'react';
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
  };
};

type SessionImage = {
  id: string;
  file_path: string;
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

type GalleryDirectoryInfo = {
  directory: string;
  is_custom: boolean;
};

type SetGalleryDirectoryOutput = {
  directory: GalleryDirectoryInfo;
  moved_paths: Array<{
    old_path: string;
    new_path: string;
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
const imageQualityOptions = ['auto', 'high', 'medium', 'low'];
const imageAspectRatioOptions = [
  {
    value: '1:1',
    defaultSize: '1024x1024',
    sizes: [
      { value: '1024x1024', label: '1024x1024' },
      { value: '2048x2048', label: '2048x2048' },
      { value: '4096x4096', label: '4096x4096 4K' },
    ],
  },
  {
    value: '1:2',
    defaultSize: '1024x2048',
    sizes: [
      { value: '1024x2048', label: '1024x2048' },
      { value: '1536x3072', label: '1536x3072' },
      { value: '2048x4096', label: '2048x4096 4K' },
    ],
  },
  {
    value: '2:1',
    defaultSize: '2048x1024',
    sizes: [
      { value: '2048x1024', label: '2048x1024' },
      { value: '3072x1536', label: '3072x1536' },
      { value: '4096x2048', label: '4096x2048 4K' },
    ],
  },
  {
    value: '9:16',
    defaultSize: '1080x1920',
    sizes: [
      { value: '1080x1920', label: '1080x1920' },
      { value: '1440x2560', label: '1440x2560' },
      { value: '2160x3840', label: '2160x3840 4K' },
    ],
  },
  {
    value: '16:9',
    defaultSize: '1920x1080',
    sizes: [
      { value: '1920x1080', label: '1920x1080' },
      { value: '2560x1440', label: '2560x1440' },
      { value: '3840x2160', label: '3840x2160 4K' },
    ],
  },
  {
    value: '3:4',
    defaultSize: '1536x2048',
    sizes: [
      { value: '1536x2048', label: '1536x2048' },
      { value: '2304x3072', label: '2304x3072' },
      { value: '3072x4096', label: '3072x4096 4K' },
    ],
  },
  {
    value: '4:3',
    defaultSize: '2048x1536',
    sizes: [
      { value: '2048x1536', label: '2048x1536' },
      { value: '3072x2304', label: '3072x2304' },
      { value: '4096x3072', label: '4096x3072 4K' },
    ],
  },
  {
    value: '名片横版',
    defaultSize: '1050x600',
    sizes: [
      { value: '1050x600', label: '1050x600' },
      { value: '2100x1200', label: '2100x1200' },
      { value: '3500x2000', label: '3500x2000' },
    ],
  },
  {
    value: '名片竖版',
    defaultSize: '600x1050',
    sizes: [
      { value: '600x1050', label: '600x1050' },
      { value: '1200x2100', label: '1200x2100' },
      { value: '2000x3500', label: '2000x3500' },
    ],
  },
];

function getAspectRatioOption(value: string) {
  return imageAspectRatioOptions.find((option) => option.value === value) ?? imageAspectRatioOptions[0];
}

function getAspectRatioForSize(value: string) {
  return imageAspectRatioOptions.find((option) => option.sizes.some((size) => size.value === value));
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

function App() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [providerForm, setProviderForm] = useState<ProviderForm>(defaultProviderForm);
  const [prompt, setPrompt] = useState('一只赛博朋克风格的橘猫坐在霓虹灯下');
  const [selectedImageModel, setSelectedImageModel] = useState('');
  const [fetchedImageModels, setFetchedImageModels] = useState<ProviderModel[]>([]);
  const [selectedImageModels, setSelectedImageModels] = useState<string[]>(defaultImageModelOptions);
  const [imageAspectRatio, setImageAspectRatio] = useState('1:1');
  const [imageSize, setImageSize] = useState('1024x1024');
  const [imageQuality, setImageQuality] = useState('auto');
  const [status, setStatus] = useState('准备就绪');
  const [settingsStatus, setSettingsStatus] = useState('');
  const [isBusy, setIsBusy] = useState(false);
  const [activeGenerationRequestId, setActiveGenerationRequestId] = useState<string | null>(null);
  const [isFetchingModels, setIsFetchingModels] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isGalleryOpen, setIsGalleryOpen] = useState(false);
  const [isUpdatingGalleryDirectory, setIsUpdatingGalleryDirectory] = useState(false);
  const [galleryInfo, setGalleryInfo] = useState<GalleryDirectoryInfo | null>(null);
  const [galleryStatus, setGalleryStatus] = useState('');
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [isUpdateOpen, setIsUpdateOpen] = useState(false);
  const autoUpdateDismissedRef = useRef(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateStatus, setUpdateStatus] = useState('');
  const [previewImage, setPreviewImage] = useState<SessionImage | null>(null);
  const [sessionImages, setSessionImages] = useState<SessionImage[]>([]);
  const [materialPaths, setMaterialPaths] = useState<string[]>([]);
  const [generationSteps, setGenerationSteps] = useState<GenerationStep[]>(initialGenerationSteps);
  const selectedAspectRatioOption = getAspectRatioOption(imageAspectRatio);
  const visibleImageModelOptions =
    selectedImageModels.length > 0 ? selectedImageModels : defaultImageModelOptions;
  const activeProviderName =
    providers.find((provider) => provider.id === providerForm.id)?.name ?? providerForm.name;
  const activeMode = materialPaths.length > 0 ? '图像编辑' : '文字生成';

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

  function updateProviderForm<K extends keyof ProviderForm>(key: K, value: ProviderForm[K]) {
    setProviderForm((current) => ({ ...current, [key]: value }));
  }

  function updateProviderKind(kind: string) {
    const option = apiKindOptions.find((item) => item.value === kind);
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

  function updateAspectRatio(value: string) {
    const option = getAspectRatioOption(value);
    setImageAspectRatio(option.value);
    setImageSize(option.defaultSize);
  }

  function updateImageSize(value: string) {
    setImageSize(value);
    const option = getAspectRatioForSize(value);
    if (option) {
      setImageAspectRatio(option.value);
    }
  }

  function updateSelectedModelRecords(modelId: string) {
    setSelectedImageModels((current) => {
      if (current.includes(modelId)) {
        if (current.length === 1) return current;
        const next = current.filter((id) => id !== modelId);
        if (selectedImageModel === modelId) {
          setSelectedImageModel(next[0] ?? '');
        }
        return next;
      }

      return [...current, modelId];
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
      const modelIds = models.map((model) => model.id);
      setFetchedImageModels(models);
      if (modelIds.length === 0) {
        setSelectedImageModels([]);
        setSelectedImageModel('');
        setStatus('未从模型列表中识别到图片模型');
        setSettingsStatus('接口可访问，但没有识别到图片模型');
        return;
      }

      setSelectedImageModels((current) => {
        const kept = current.filter((model) => modelIds.includes(model));
        const next = [...kept, ...modelIds.filter((model) => !kept.includes(model))];
        setSelectedImageModel((selected) => (next.includes(selected) ? selected : (next[0] ?? '')));
        return next;
      });
      setStatus(`已获取 ${modelIds.length} 个图片模型`);
      setSettingsStatus(`已获取 ${modelIds.length} 个图片模型`);
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
      const storedSelectedModels =
        capabilities.selected_image_models?.filter((model) =>
          storedModels.some((storedModel) => storedModel.id === model),
        ) ?? [];
      const nextSelectedModels =
        storedSelectedModels.length > 0
          ? storedSelectedModels
          : storedModels.length > 0
            ? storedModels.map((model) => model.id)
            : defaultImageModelOptions;
      setFetchedImageModels(storedModels);
      setSelectedImageModels(nextSelectedModels);
      setSelectedImageModel((model) =>
        nextSelectedModels.includes(model) ? model : (nextSelectedModels[0] ?? ''),
      );
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
    const storedSelectedModels =
      capabilities.selected_image_models?.filter((model) =>
        storedModels.some((storedModel) => storedModel.id === model),
      ) ?? [];
    const nextSelectedModels =
      storedSelectedModels.length > 0
        ? storedSelectedModels
        : storedModels.length > 0
          ? storedModels.map((model) => model.id)
          : defaultImageModelOptions;
    setFetchedImageModels(storedModels);
    setSelectedImageModels(nextSelectedModels);
    setSelectedImageModel((model) =>
      nextSelectedModels.includes(model) ? model : (nextSelectedModels[0] ?? ''),
    );
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
    setIsBusy(true);
    const requestId = createRequestId();
    setActiveGenerationRequestId(requestId);
    setGenerationSteps(initialGenerationSteps);
    setStatus('正在生成图片...');
    try {
      startStep(0);
      await new Promise((resolve) => window.setTimeout(resolve, 80));
      startStep(1);
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      startStep(2);
      const result = await invoke<GenerateImageOutput>('generate_image', {
        input: {
          provider_id: providerForm.id,
          request_id: requestId,
          prompt,
          model: selectedImageModel,
          size: imageSize,
          quality: imageQuality,
          image_paths: materialPaths,
        },
      });
      startStep(3);
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      startStep(4);
      await refreshProviders();
      setSessionImages((images) => [
        {
          id: result.asset.id,
          file_path: result.asset.file_path,
          prompt,
          created_at: new Date().toLocaleString(),
        },
        ...images,
      ]);
      setStep(4, 'done');
      setStatus(`生成完成：${result.asset.file_path}`);
    } catch (error) {
      setGenerationSteps((steps) =>
        steps.map((step) => (step.status === 'active' ? { ...step, status: 'error' } : step)),
      );
      const message = formatError(error);
      setStatus(message.includes('生成已强制停止') ? '已强制停止生成' : `生成失败：${message}`);
    } finally {
      setIsBusy(false);
      setActiveGenerationRequestId(null);
    }
  }

  async function stopGeneration() {
    if (!activeGenerationRequestId) return;
    setStatus('正在强制停止生成...');
    try {
      await invoke('cancel_generation', { requestId: activeGenerationRequestId });
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
      setMaterialPaths((current) => Array.from(new Set([...current, ...paths])));
      setStatus(`已导入 ${paths.length} 张素材`);
    } catch (error) {
      setStatus(`打开素材选择器失败：${formatError(error)}`);
    }
  }

  function removeMaterialImage(path: string) {
    setMaterialPaths((current) => current.filter((item) => item !== path));
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
        const movedPathMap = new Map(result.moved_paths.map((path) => [path.old_path, path.new_path]));
        setSessionImages((images) =>
          images.map((image) => ({
            ...image,
            file_path: movedPathMap.get(image.file_path) ?? image.file_path,
          })),
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

  useEffect(() => {
    refreshProviders().catch(() => setStatus('后端未启动或数据库初始化失败'));
    checkForUpdates({ silent: true, autoOpen: true }).catch(() => undefined);
  }, []);

  return (
    <main className="app-shell">
      <aside className="side-rail">
        <div className="rail-logo">
          <img src={appLogo} alt="Image Draw AI" />
        </div>
        <nav className="rail-nav" aria-label="主导航">
          <button className="rail-button active" title="生成">
            <span className="rail-icon"><RobotOutlined /></span>
            <strong>生成</strong>
          </button>
          <button className="rail-button" title="素材" onClick={pickMaterialImages} disabled={isBusy}>
            <span className="rail-icon"><PictureOutlined /></span>
            <strong>素材</strong>
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
              <small>{imageAspectRatio} / {imageSize} / {imageQuality}</small>
            </div>

            <label className="field prompt-field">
              <span>提示词</span>
              <textarea value={prompt} onChange={(event) => setPrompt(event.target.value)} />
            </label>

            <div className="material-panel">
              <div className="material-header">
                <div>
                  <strong>参考图</strong>
                  <span>{materialPaths.length > 0 ? `${materialPaths.length} 张素材` : '未导入'}</span>
                </div>
                {materialPaths.length > 0 && (
                  <button className="ghost mini" onClick={() => setMaterialPaths([])} disabled={isBusy}>清空</button>
                )}
              </div>

              <div className="reference-strip">
                <button className="add-reference-card" onClick={pickMaterialImages} disabled={isBusy}>
                  <span>+</span>
                  <strong>参考图</strong>
                  <small>PNG/JPG/WEBP</small>
                </button>
                {materialPaths.map((path, index) => (
                  <article className="reference-card" key={path}>
                    <img src={convertFileSrc(path)} alt="素材图片" />
                    <span>{index + 1}</span>
                    <button onClick={() => removeMaterialImage(path)} disabled={isBusy}>×</button>
                  </article>
                ))}
              </div>
            </div>

            <div className="params-card">
              <div className="section-heading">
                <div>
                  <span>生成参数</span>
                  <strong>基础</strong>
                </div>
              </div>
              <div className="params-grid">
                <label className="field compact-field">
                  <span>图像模型</span>
                  <select value={selectedImageModel} onChange={(event) => setSelectedImageModel(event.target.value)} disabled={isBusy}>
                    {visibleImageModelOptions.length === 0 && <option value="">未获取模型</option>}
                    {visibleImageModelOptions.map((model) => (
                      <option key={model} value={model}>{model}</option>
                    ))}
                  </select>
                </label>
                <label className="field compact-field">
                  <span>质量</span>
                  <select value={imageQuality} onChange={(event) => setImageQuality(event.target.value)} disabled={isBusy}>
                    {imageQualityOptions.map((quality) => (
                      <option key={quality} value={quality}>{quality}</option>
                    ))}
                  </select>
                </label>
                <label className="field compact-field">
                  <span>比例</span>
                  <select value={imageAspectRatio} onChange={(event) => updateAspectRatio(event.target.value)} disabled={isBusy}>
                    {imageAspectRatioOptions.map((option) => (
                      <option key={option.value} value={option.value}>{option.value}</option>
                    ))}
                  </select>
                </label>
                <label className="field compact-field">
                  <span>分辨率</span>
                  <select value={imageSize} onChange={(event) => updateImageSize(event.target.value)} disabled={isBusy}>
                    {selectedAspectRatioOption.sizes.map((size) => (
                      <option key={size.value} value={size.value}>{size.label}</option>
                    ))}
                  </select>
                </label>
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
                disabled={!isBusy || !activeGenerationRequestId}
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
                <p>{selectedImageModel || '未获取模型'} / {imageSize} / {imageQuality}</p>
              </div>
            ) : (
              <div className="image-grid">
                {sessionImages.map((image) => (
                  <article className="image-card" key={image.id}>
                    <button className="image-preview-button" onClick={() => setPreviewImage(image)}>
                      <img src={convertFileSrc(image.file_path)} alt={image.prompt} />
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
                    {fetchedImageModels.map((model) => (
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
                  onClick={() => updateInfo?.asset && openUpdateUrl(updateInfo.asset.download_url)}
                  disabled={!updateInfo?.asset}
                >
                  <CloudDownloadOutlined />
                  下载更新
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
            <img src={convertFileSrc(previewImage.file_path)} alt={previewImage.prompt} />
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
