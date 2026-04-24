CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  base_url TEXT NOT NULL,
  api_key_encrypted TEXT,
  text_model TEXT,
  image_model TEXT,
  capabilities TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS generation_tasks (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  task_type TEXT NOT NULL,
  prompt TEXT NOT NULL,
  negative_prompt TEXT,
  model TEXT NOT NULL,
  size TEXT,
  quality TEXT,
  status TEXT NOT NULL,
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  finished_at TEXT,
  FOREIGN KEY(provider_id) REFERENCES providers(id)
);

CREATE TABLE IF NOT EXISTS image_assets (
  id TEXT PRIMARY KEY,
  task_id TEXT,
  file_path TEXT NOT NULL,
  thumbnail_path TEXT,
  mime_type TEXT,
  width INTEGER,
  height INTEGER,
  file_size INTEGER,
  source_type TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES generation_tasks(id)
);

CREATE TABLE IF NOT EXISTS conversations (
  id TEXT PRIMARY KEY,
  title TEXT,
  provider_id TEXT NOT NULL,
  model TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(provider_id) REFERENCES providers(id)
);

CREATE TABLE IF NOT EXISTS ai_request_logs (
  id TEXT PRIMARY KEY,
  task_id TEXT,
  provider_id TEXT NOT NULL,
  endpoint TEXT NOT NULL,
  request_summary TEXT,
  response_summary TEXT,
  status_code INTEGER,
  latency_ms INTEGER,
  created_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES generation_tasks(id),
  FOREIGN KEY(provider_id) REFERENCES providers(id)
);
