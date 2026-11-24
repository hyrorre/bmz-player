export interface ScoreClient {
  client_name: string | null
  client_version: string | null
}

export interface ScoreReplay {
  status: string
  size_bytes: number | null
  format: string
}
