export interface DockerPort {
  ip: string | null
  port_type: string
  private_port: number
  public_port: number | null
}

export interface DockerContainer {
  created: number
  id: string
  image: string
  labels: Record<string, string>
  name: string
  ports: DockerPort[]
  state: string
  status: string
}

export interface DockerContainerStats {
  block_read: number
  block_write: number
  cpu_percent: number
  id: string
  memory_limit: number
  memory_percent: number
  memory_usage: number
  name: string
  network_rx: number
  network_tx: number
}

export interface DockerLogEntry {
  message: string
  stream: string
  timestamp: string | null
}

export interface DockerEventInfo {
  action: string
  actor_id: string
  actor_name: string | null
  attributes: Record<string, string>
  event_type: string
  timestamp: number
}

export interface DockerSystemInfo {
  api_version: string
  arch: string
  containers_paused: number
  containers_running: number
  containers_stopped: number
  docker_version: string
  images: number
  memory_total: number
  os: string
}

export interface DockerNetwork {
  containers: Record<string, string>
  driver: string
  id: string
  name: string
  scope: string
}

export interface DockerVolume {
  created_at: string | null
  driver: string
  labels: Record<string, string>
  mountpoint: string
  name: string
}
