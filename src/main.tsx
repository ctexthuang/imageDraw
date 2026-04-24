import React, { useEffect, useState } from 'react';
import ReactDOM from 'react-dom/client';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
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
  text_model: string;
  image_model: string;
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
  text_model: 'gpt-5',
  image_model: 'gpt-image-2',
  enabled: true,
};

const initialGenerationSteps: GenerationStep[] = [
  { label: '保存 Provider 配置', status: 'pending' },
  { label: '提交生成任务', status: 'pending' },
  { label: '请求并等待图像模型返回', status: 'pending' },
  { label: '保存图片到应用数据文件夹', status: 'pending' },
  { label: '更新本次打开图片列表', status: 'pending' },
];

function App() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [providerForm, setProviderForm] = useState<ProviderForm>(defaultProviderForm);
  const [prompt, setPrompt] = useState('一只赛博朋克风格的橘猫坐在霓虹灯下');
  const [status, setStatus] = useState('准备就绪');
  const [isBusy, setIsBusy] = useState(false);
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
        text_model: current.text_model ?? defaultProviderForm.text_model,
        image_model: current.image_model ?? defaultProviderForm.image_model,
        enabled: current.enabled,
      }));
    }
  }

  async function saveProvider() {
    setIsBusy(true);
    setStatus('正在保存 Provider 配置...');
    try {
      await invoke('upsert_provider', {
        input: providerForm,
      });
      await refreshProviders();
      setStatus('Provider 已保存，可以直接生成图片');
    } catch (error) {
      setStatus(`保存失败：${formatError(error)}`);
    } finally {
      setIsBusy(false);
    }
  }

  async function deleteProvider(id: string) {
    setIsBusy(true);
    setStatus('正在删除 Provider...');
    try {
      await invoke('delete_provider', { id });
      await refreshProviders();
      if (providerForm.id === id) {
        setProviderForm(defaultProviderForm);
      }
      setStatus('Provider 已删除');
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
      text_model: provider.text_model ?? defaultProviderForm.text_model,
      image_model: provider.image_model ?? defaultProviderForm.image_model,
      enabled: provider.enabled,
    }));
    setStatus('已载入 Provider，修改后点击保存即可覆盖当前配置。');
  }

  async function generateImage() {
    setIsBusy(true);
    setGenerationSteps(initialGenerationSteps);
    setStatus('正在生成图片...');
    try {
      startStep(0);
      await invoke('upsert_provider', {
        input: providerForm,
      });
      startStep(1);
      await new Promise((resolve) => window.setTimeout(resolve, 120));
      startStep(2);
      const result = await invoke<GenerateImageOutput>('generate_image', {
        input: {
          provider_id: providerForm.id,
          prompt,
          model: providerForm.image_model,
          size: '1024x1024',
          quality: 'auto',
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
      setStatus(`生成完成，已保存到应用数据文件夹：${result.asset.file_path}`);
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
    setStatus('正在打开素材图片选择器...');
    try {
      const paths = await invoke<string[]>('pick_material_images');
      if (paths.length === 0) {
        setStatus('未选择素材图片');
        return;
      }
      setMaterialPaths((current) => Array.from(new Set([...current, ...paths])));
      setStatus(`已导入 ${paths.length} 张素材图片`);
    } catch (error) {
      setStatus(`打开素材选择器失败：${formatError(error)}`);
    }
  }

  function removeMaterialImage(path: string) {
    setMaterialPaths((current) => current.filter((item) => item !== path));
  }

  useEffect(() => {
    refreshProviders().catch(() => setStatus('后端未启动或数据库初始化失败'));
  }, []);

  return (
    <main className="app">
      <section className="hero">
        <h1>Image Draw AI</h1>
        <p>图片默认保存到应用数据文件夹。</p>
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Provider</p>
            <h2>模型供应商配置</h2>
          </div>
          <button onClick={refreshProviders} disabled={isBusy}>刷新</button>
        </div>

        <div className="grid two">
          <label>
            配置 ID
            <input
              value={providerForm.id}
              onChange={(event) => updateProviderForm('id', event.target.value)}
              placeholder="default-openai"
            />
          </label>
          <label>
            名称
            <input
              value={providerForm.name}
              onChange={(event) => updateProviderForm('name', event.target.value)}
              placeholder="OpenAI / 中转站"
            />
          </label>
        </div>

        <label>
          Base URL
          <input
            value={providerForm.base_url}
            onChange={(event) => updateProviderForm('base_url', event.target.value)}
            placeholder="https://api.openai.com/v1"
          />
          <small>填写 API 地址，不是中转站网页地址；通常以 /v1 结尾。</small>
        </label>

        <label>
          API Key
          <input
            value={providerForm.api_key}
            onChange={(event) => updateProviderForm('api_key', event.target.value)}
            placeholder="sk-... 或中转站 key"
            type="password"
          />
        </label>

        <div className="grid two">
          <label>
            文本模型
            <input
              value={providerForm.text_model}
              onChange={(event) => updateProviderForm('text_model', event.target.value)}
              placeholder="gpt-5"
            />
          </label>
          <label>
            图像模型
            <input
              value={providerForm.image_model}
              onChange={(event) => updateProviderForm('image_model', event.target.value)}
              placeholder="gpt-image-2"
            />
          </label>
        </div>

        <div className="row">
          <button onClick={saveProvider} disabled={isBusy}>保存 Provider</button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Generate</p>
            <h2>生成图片</h2>
          </div>
        </div>
        <textarea value={prompt} onChange={(event) => setPrompt(event.target.value)} />
        <div className="material-toolbar">
          <button onClick={pickMaterialImages} disabled={isBusy}>导入素材图片</button>
          {materialPaths.length > 0 && (
            <button onClick={() => setMaterialPaths([])} disabled={isBusy}>清空素材</button>
          )}
          <span>{materialPaths.length > 0 ? `已选择 ${materialPaths.length} 张素材，将使用图像编辑模式` : '未选择素材，将使用文生图模式'}</span>
        </div>

        {materialPaths.length > 0 && (
          <div className="material-grid">
            {materialPaths.map((path) => (
              <article className="material-card" key={path}>
                <img src={convertFileSrc(path)} alt="素材图片" />
                <button onClick={() => removeMaterialImage(path)} disabled={isBusy}>移除</button>
                <span>{path}</span>
              </article>
            ))}
          </div>
        )}

        <button className="primary" onClick={generateImage} disabled={isBusy}>
          {isBusy ? '正在生成...' : '生成图片'}
        </button>
        <div className={`progress-card ${isBusy ? 'is-loading' : ''}`}>
          <div className="spinner" aria-hidden="true" />
          <div className="progress-content">
            <strong>{isBusy ? '生成流程进行中' : '生成流程'}</strong>
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
        <p className="status">{status}</p>
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Session</p>
            <h2>本次打开生成的图片</h2>
          </div>
          <span className="count">{sessionImages.length} 张</span>
        </div>
        {sessionImages.length === 0 ? (
          <p>当前打开周期内还没有生成图片。</p>
        ) : (
          <div className="image-grid">
            {sessionImages.map((image) => (
              <article className="image-card" key={image.id}>
                <img src={convertFileSrc(image.file_path)} alt={image.prompt} />
                <div>
                  <strong>{image.created_at}</strong>
                  <p>{image.prompt}</p>
                  <span>{image.file_path}</span>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <h2>已保存 Provider</h2>
        {providers.length === 0 ? (
          <p>暂无 Provider，请先保存配置。</p>
        ) : (
          <ul className="provider-list">
            {providers.map((provider) => (
              <li key={provider.id}>
                <div className="provider-item">
                  <button className="link-button" onClick={() => loadProvider(provider)} disabled={isBusy}>
                    <strong>{provider.name}</strong>
                    <span>{provider.base_url}</span>
                    <span>{provider.image_model}</span>
                  </button>
                  <button className="danger" onClick={() => deleteProvider(provider.id)} disabled={isBusy}>
                    删除
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(<App />);
