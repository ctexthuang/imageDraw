import React, { useEffect, useState } from 'react';
import ReactDOM from 'react-dom/client';
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

type GenerationStep = {
  label: string;
  status: 'pending' | 'active' | 'done' | 'error';
};

function formatError(error: unknown) {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return JSON.stringify(error);
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

const initialGenerationSteps: GenerationStep[] = [
  { label: '保存配置', status: 'pending' },
  { label: '提交任务', status: 'pending' },
  { label: '等待模型返回', status: 'pending' },
  { label: '保存到应用文件夹', status: 'pending' },
  { label: '更新结果列表', status: 'pending' },
];

const imageModelOptions = ['gpt-image-1', 'gpt-image-1.5', 'gpt-image-2'];
const imageSizeOptions = ['1024x1024', '1024x1536', '1536x1024'];
const imageQualityOptions = ['auto', 'high', 'medium', 'low'];

function App() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [providerForm, setProviderForm] = useState<ProviderForm>(defaultProviderForm);
  const [prompt, setPrompt] = useState('一只赛博朋克风格的橘猫坐在霓虹灯下');
  const [selectedImageModel, setSelectedImageModel] = useState('gpt-image-2');
  const [imageSize, setImageSize] = useState('1024x1024');
  const [imageQuality, setImageQuality] = useState('auto');
  const [status, setStatus] = useState('准备就绪');
  const [isBusy, setIsBusy] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [previewImage, setPreviewImage] = useState<SessionImage | null>(null);
  const [sessionImages, setSessionImages] = useState<SessionImage[]>([]);
  const [materialPaths, setMaterialPaths] = useState<string[]>([]);
  const [generationSteps, setGenerationSteps] = useState<GenerationStep[]>(initialGenerationSteps);

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

  async function refreshProviders() {
    const result = await invoke<ProviderConfig[]>('list_providers');
    setProviders(result);
    const current = result.find((provider) => provider.id === providerForm.id) ?? result[0];
    if (current) {
      setProviderForm((form) => ({
        ...form,
        id: current.id,
        name: current.name,
        kind: current.kind,
        base_url: current.base_url,
        api_key: current.api_key ?? form.api_key,
        text_model: current.text_model ?? null,
        image_model: null,
        enabled: current.enabled,
      }));
    }
  }

  async function saveProvider() {
    setIsBusy(true);
    setStatus('正在保存配置...');
    try {
      await invoke('upsert_provider', { input: { ...providerForm, image_model: null } });
      await refreshProviders();
      setStatus('配置已保存');
    } catch (error) {
      setStatus(`保存失败：${formatError(error)}`);
    } finally {
      setIsBusy(false);
    }
  }

  async function deleteProvider(id: string) {
    setIsBusy(true);
    setStatus('正在删除配置...');
    try {
      await invoke('delete_provider', { id });
      await refreshProviders();
      if (providerForm.id === id) {
        setProviderForm(defaultProviderForm);
      }
      setStatus('配置已删除');
    } catch (error) {
      setStatus(`删除失败：${formatError(error)}`);
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
      api_key: provider.api_key ?? form.api_key,
      text_model: provider.text_model ?? null,
      image_model: null,
      enabled: provider.enabled,
    }));
    setStatus('已切换模型配置');
  }

  async function generateImage() {
    setIsBusy(true);
    setGenerationSteps(initialGenerationSteps);
    setStatus('正在生成图片...');
    try {
      startStep(0);
      await invoke('upsert_provider', { input: { ...providerForm, image_model: null } });
      startStep(1);
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      startStep(2);
      const result = await invoke<GenerateImageOutput>('generate_image', {
        input: {
          provider_id: providerForm.id,
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
      setStatus(`生成失败：${formatError(error)}`);
    } finally {
      setIsBusy(false);
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

  useEffect(() => {
    refreshProviders().catch(() => setStatus('后端未启动或数据库初始化失败'));
  }, []);

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <img className="brand-mark" src={appLogo} alt="Image Draw AI" />
          <div>
            <h1>Image Draw AI</h1>
            <p>图片默认保存到应用数据文件夹</p>
          </div>
        </div>
        <div className="topbar-actions">
          <div className="current-provider">
            <span>当前模型</span>
            <strong>{selectedImageModel}</strong>
          </div>
          <button className="ghost" onClick={() => setIsSettingsOpen(true)}>设置</button>
        </div>
      </header>

      <section className="workspace">
        <aside className="compose-card">
          <div className="section-heading">
            <span>创作区</span>
            <strong>{materialPaths.length > 0 ? '素材生成' : '文字生成'}</strong>
          </div>

          <label className="field prompt-field">
            <span>提示词</span>
            <textarea value={prompt} onChange={(event) => setPrompt(event.target.value)} />
          </label>

          <div className="material-panel">
            <div className="material-header">
              <div>
                <strong>参考图</strong>
                <span>{materialPaths.length > 0 ? `${materialPaths.length} 张，图像编辑模式` : '可选，支持多张'}</span>
              </div>
              {materialPaths.length > 0 && (
                <button className="ghost mini" onClick={() => setMaterialPaths([])} disabled={isBusy}>清空</button>
              )}
            </div>

            <div className="reference-strip">
              <button className="add-reference-card" onClick={pickMaterialImages} disabled={isBusy}>
                <span>+</span>
                <strong>添加参考图</strong>
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
              <span>生成参数</span>
              <strong>基础</strong>
            </div>
            <label className="field compact-field">
              <span>图像模型</span>
              <select value={selectedImageModel} onChange={(event) => setSelectedImageModel(event.target.value)} disabled={isBusy}>
                {imageModelOptions.map((model) => (
                  <option key={model} value={model}>{model}</option>
                ))}
              </select>
            </label>
            <div className="grid two">
              <label className="field compact-field">
                <span>分辨率</span>
                <select value={imageSize} onChange={(event) => setImageSize(event.target.value)} disabled={isBusy}>
                  {imageSizeOptions.map((size) => (
                    <option key={size} value={size}>{size}</option>
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
            </div>
          </div>

          <button className="generate-button" onClick={generateImage} disabled={isBusy}>
            {isBusy ? '正在生成...' : '开始生成'}
          </button>

          {status !== '准备就绪' && <p className="status">{status}</p>}
        </aside>

        <section className="result-card">
          <div className="section-heading result-heading">
            <span>结果区</span>
            <div className="heading-actions">
              <strong>本次生成 {sessionImages.length} 张</strong>
              <button className="ghost mini" onClick={openGeneratedDir}>打开目录</button>
            </div>
          </div>

          {sessionImages.length === 0 ? (
            <div className="empty-state">
              <div>暂无图片</div>
              <p>输入提示词，点击开始生成后会显示在这里。</p>
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
              <section className="settings-group">
                <div className="section-heading">
                  <span>基础信息</span>
                  <strong>配置名称</strong>
                </div>
                <div className="grid two">
                  <label className="field">
                    <span>配置 ID</span>
                    <input value={providerForm.id} onChange={(event) => updateProviderForm('id', event.target.value)} />
                  </label>
                  <label className="field">
                    <span>名称</span>
                    <input value={providerForm.name} onChange={(event) => updateProviderForm('name', event.target.value)} />
                  </label>
                </div>
              </section>

              <section className="settings-group">
                <div className="section-heading">
                  <span>接口信息</span>
                  <strong>中转站 / OpenAI</strong>
                </div>
                <label className="field">
                  <span>Base URL</span>
                  <input
                    value={providerForm.base_url}
                    onChange={(event) => updateProviderForm('base_url', event.target.value)}
                    placeholder="https://api.openai.com/v1"
                  />
                  <small>填写 API 地址，不是中转站网页地址；通常以 /v1 结尾。</small>
                </label>

                <label className="field">
                  <span>API Key</span>
                  <input
                    value={providerForm.api_key}
                    onChange={(event) => updateProviderForm('api_key', event.target.value)}
                    placeholder="sk-... 或中转站 key"
                    type="password"
                  />
                </label>
              </section>

              <div className="drawer-actions">
                <button onClick={saveProvider} disabled={isBusy}>保存配置</button>
                <button className="ghost" onClick={refreshProviders} disabled={isBusy}>刷新</button>
              </div>

              <div className="saved-providers">
                <div className="section-heading">
                  <span>已保存</span>
                  <strong>{providers.length} 个配置</strong>
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
