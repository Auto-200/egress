import type { AuditEvent, Line, LineDetail, Node, Overview, Region, User } from './types';

export class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body && !headers.has('content-type')) {
    headers.set('content-type', 'application/json');
  }
  const response = await fetch(`/api/v1${path}`, {
    ...init,
    headers,
    credentials: 'include',
  });
  const text = await response.text();
  let data: unknown = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = null;
    }
  }
  if (!response.ok) {
    const message =
      typeof data === 'object' && data && 'error' in data
        ? String((data as { error: unknown }).error)
        : response.statusText;
    throw new ApiError(response.status, message);
  }
  return data as T;
}

function jsonBody(body: unknown): string {
  return JSON.stringify(body);
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, body: unknown) =>
    request<T>(path, { method: 'POST', body: jsonBody(body) }),
  patch: <T>(path: string, body: unknown) =>
    request<T>(path, { method: 'PATCH', body: jsonBody(body) }),
  put: <T>(path: string, body: unknown) =>
    request<T>(path, { method: 'PUT', body: jsonBody(body) }),
  delete: (path: string) => request<void>(path, { method: 'DELETE' }),
};

export const resources = {
  login: (username: string, password: string) =>
    api.post<User>('/auth/login', { username, password }),
  logout: () => api.post<User>('/auth/logout', {}),
  me: () => api.get<User>('/auth/me'),
  overview: () => api.get<Overview>('/overview'),
  regions: () => api.get<Region[]>('/regions'),
  createRegion: (body: unknown) => api.post<{ id: string }>('/regions', body),
  updateRegion: (id: string, body: unknown) => api.patch<Region>(`/regions/${id}`, body),
  deleteRegion: (id: string) => api.delete(`/regions/${id}`),
  nodes: () => api.get<Node[]>('/nodes'),
  createNode: (body: unknown) => api.post<{ id: string }>('/nodes', body),
  updateNode: (id: string, body: unknown) => api.patch<{ id: string }>(`/nodes/${id}`, body),
  deleteNode: (id: string) => api.delete(`/nodes/${id}`),
  lines: () => api.get<Line[]>('/lines'),
  line: (id: string) => api.get<LineDetail>(`/lines/${id}`),
  createLine: (body: unknown) => api.post<{ id: string }>('/lines', body),
  updateLine: (id: string, body: unknown) => api.patch<{ id: string }>(`/lines/${id}`, body),
  deleteLine: (id: string) => api.delete(`/lines/${id}`),
  setLineNodes: (id: string, nodes: unknown[]) =>
    api.put<void>(`/lines/${id}/nodes`, { nodes }),
  audit: (limit = 200) => api.get<AuditEvent[]>(`/audit?limit=${limit}`),
};
