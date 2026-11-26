import type { IrDeviceType, IrJudges, IrVerificationStatus } from '../../shared/types/ir'

export interface ScoreClient {
  client_name: string | null
  client_version: string | null
}

export interface ScoreReplay {
  status: string
  size_bytes: number | null
  format: string
}

export interface ScoreDetailResponse {
  score: ScoreClient & {
    id: string
    judges: Partial<IrJudges> | null
    device_type: IrDeviceType
    played_at: string | null
    server_received_at: string
    verification: IrVerificationStatus
  }
  replay: ScoreReplay | null
}
