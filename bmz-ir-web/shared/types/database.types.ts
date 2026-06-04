export type Json =
  | string
  | number
  | boolean
  | null
  | { [key: string]: Json | undefined }
  | Json[]

export type Database = {
  graphql_public: {
    Tables: {
      [_ in never]: never
    }
    Views: {
      [_ in never]: never
    }
    Functions: {
      graphql: {
        Args: {
          extensions?: Json
          operationName?: string
          query?: string
          variables?: Json
        }
        Returns: Json
      }
    }
    Enums: {
      [_ in never]: never
    }
    CompositeTypes: {
      [_ in never]: never
    }
  }
  public: {
    Tables: {
      best_scores: {
        Row: {
          chart_sha256: string
          clear_rank: number
          clear_type: string
          device_type: string
          effective_ln_mode: string
          ex_score: number
          gauge: string
          id: string
          ln_policy: string
          max_combo: number
          min_bp: number
          min_cb: number
          played_at: string | null
          player_id: string
          score_id: string
          scoring: string
          server_received_at: string
          updated_at: string
          verification: string
        }
        Insert: {
          chart_sha256: string
          clear_rank: number
          clear_type: string
          device_type?: string
          effective_ln_mode: string
          ex_score: number
          gauge: string
          id?: string
          ln_policy: string
          max_combo: number
          min_bp: number
          min_cb: number
          played_at?: string | null
          player_id: string
          score_id: string
          scoring: string
          server_received_at: string
          updated_at?: string
          verification?: string
        }
        Update: {
          chart_sha256?: string
          clear_rank?: number
          clear_type?: string
          device_type?: string
          effective_ln_mode?: string
          ex_score?: number
          gauge?: string
          id?: string
          ln_policy?: string
          max_combo?: number
          min_bp?: number
          min_cb?: number
          played_at?: string | null
          player_id?: string
          score_id?: string
          scoring?: string
          server_received_at?: string
          updated_at?: string
          verification?: string
        }
        Relationships: [
          {
            foreignKeyName: "best_scores_chart_sha256_fkey"
            columns: ["chart_sha256"]
            isOneToOne: false
            referencedRelation: "charts"
            referencedColumns: ["sha256"]
          },
          {
            foreignKeyName: "best_scores_player_id_fkey"
            columns: ["player_id"]
            isOneToOne: false
            referencedRelation: "profiles"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "best_scores_score_id_fkey"
            columns: ["score_id"]
            isOneToOne: false
            referencedRelation: "scores"
            referencedColumns: ["id"]
          },
        ]
      }
      charts: {
        Row: {
          append_url: string | null
          artist: string | null
          cn_notes: number
          created_at: string
          genre: string | null
          has_cn: boolean
          has_defined_cn: boolean
          has_defined_hcn: boolean
          has_defined_ln: boolean
          has_hcn: boolean
          has_ln: boolean
          has_mine: boolean
          has_random: boolean
          has_stop: boolean
          has_undefined_ln: boolean
          hcn_notes: number
          headers: Json
          judge_rank: number | null
          level: number | null
          ln_notes: number
          max_bpm: number | null
          md5: string | null
          min_bpm: number | null
          mine_notes: number
          mode: string
          notes: number
          sha256: string
          source_url: string | null
          subartists: string[]
          subtitle: string | null
          title: string
          total: number | null
          updated_at: string
        }
        Insert: {
          append_url?: string | null
          artist?: string | null
          cn_notes?: number
          created_at?: string
          genre?: string | null
          has_cn?: boolean
          has_defined_cn?: boolean
          has_defined_hcn?: boolean
          has_defined_ln?: boolean
          has_hcn?: boolean
          has_ln?: boolean
          has_mine?: boolean
          has_random?: boolean
          has_stop?: boolean
          has_undefined_ln?: boolean
          hcn_notes?: number
          headers?: Json
          judge_rank?: number | null
          level?: number | null
          ln_notes?: number
          max_bpm?: number | null
          md5?: string | null
          min_bpm?: number | null
          mine_notes?: number
          mode: string
          notes?: number
          sha256: string
          source_url?: string | null
          subartists?: string[]
          subtitle?: string | null
          title?: string
          total?: number | null
          updated_at?: string
        }
        Update: {
          append_url?: string | null
          artist?: string | null
          cn_notes?: number
          created_at?: string
          genre?: string | null
          has_cn?: boolean
          has_defined_cn?: boolean
          has_defined_hcn?: boolean
          has_defined_ln?: boolean
          has_hcn?: boolean
          has_ln?: boolean
          has_mine?: boolean
          has_random?: boolean
          has_stop?: boolean
          has_undefined_ln?: boolean
          hcn_notes?: number
          headers?: Json
          judge_rank?: number | null
          level?: number | null
          ln_notes?: number
          max_bpm?: number | null
          md5?: string | null
          min_bpm?: number | null
          mine_notes?: number
          mode?: string
          notes?: number
          sha256?: string
          source_url?: string | null
          subartists?: string[]
          subtitle?: string | null
          title?: string
          total?: number | null
          updated_at?: string
        }
        Relationships: []
      }
      device_keys: {
        Row: {
          algorithm: string
          created_at: string
          id: string
          player_id: string
          public_key: string
          revoked_at: string | null
        }
        Insert: {
          algorithm?: string
          created_at?: string
          id?: string
          player_id: string
          public_key: string
          revoked_at?: string | null
        }
        Update: {
          algorithm?: string
          created_at?: string
          id?: string
          player_id?: string
          public_key?: string
          revoked_at?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "device_keys_player_id_fkey"
            columns: ["player_id"]
            isOneToOne: false
            referencedRelation: "profiles"
            referencedColumns: ["id"]
          },
        ]
      }
      profiles: {
        Row: {
          bio: string
          created_at: string
          display_name: string
          id: string
          updated_at: string
        }
        Insert: {
          bio?: string
          created_at?: string
          display_name?: string
          id: string
          updated_at?: string
        }
        Update: {
          bio?: string
          created_at?: string
          display_name?: string
          id?: string
          updated_at?: string
        }
        Relationships: []
      }
      replay_objects: {
        Row: {
          created_at: string
          format: string
          hash: string
          id: string
          object_path: string | null
          player_id: string
          score_id: string
          size_bytes: number | null
          status: string
          updated_at: string
        }
        Insert: {
          created_at?: string
          format: string
          hash: string
          id?: string
          object_path?: string | null
          player_id: string
          score_id: string
          size_bytes?: number | null
          status?: string
          updated_at?: string
        }
        Update: {
          created_at?: string
          format?: string
          hash?: string
          id?: string
          object_path?: string | null
          player_id?: string
          score_id?: string
          size_bytes?: number | null
          status?: string
          updated_at?: string
        }
        Relationships: [
          {
            foreignKeyName: "replay_objects_player_id_fkey"
            columns: ["player_id"]
            isOneToOne: false
            referencedRelation: "profiles"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "replay_objects_score_id_fkey"
            columns: ["score_id"]
            isOneToOne: false
            referencedRelation: "scores"
            referencedColumns: ["id"]
          },
        ]
      }
      rival_relationships: {
        Row: {
          created_at: string
          owner_player_id: string
          relation_type: string
          target_player_id: string
        }
        Insert: {
          created_at?: string
          owner_player_id: string
          relation_type?: string
          target_player_id: string
        }
        Update: {
          created_at?: string
          owner_player_id?: string
          relation_type?: string
          target_player_id?: string
        }
        Relationships: [
          {
            foreignKeyName: "rival_relationships_owner_player_id_fkey"
            columns: ["owner_player_id"]
            isOneToOne: false
            referencedRelation: "profiles"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "rival_relationships_target_player_id_fkey"
            columns: ["target_player_id"]
            isOneToOne: false
            referencedRelation: "profiles"
            referencedColumns: ["id"]
          },
        ]
      }
      scores: {
        Row: {
          accepted: boolean
          avg_judge_ms: number | null
          bp: number
          cb: number
          chart_sha256: string
          clear_rank: number
          clear_type: string
          client_name: string
          client_version: string
          created_at: string
          device_type: string
          duration_ms: number | null
          effective_ln_mode: string
          evidence: Json
          ex_score: number
          gauge: string
          id: string
          idempotency_key: string
          judge_algorithm: string
          judges: Json
          key_mode: string
          ln_policy: string
          max_combo: number
          min_bp: number
          min_cb: number
          notes: number
          pass_notes: number
          platform: string
          play_mode: string
          play_options: Json
          played_at: string | null
          player_id: string
          rejection_reason: string | null
          replay_format: string | null
          replay_hash: string | null
          replay_upload_intent: string | null
          scoring: string
          server_received_at: string
          verification: string
        }
        Insert: {
          accepted?: boolean
          avg_judge_ms?: number | null
          bp: number
          cb: number
          chart_sha256: string
          clear_rank: number
          clear_type: string
          client_name: string
          client_version: string
          created_at?: string
          device_type?: string
          duration_ms?: number | null
          effective_ln_mode: string
          evidence?: Json
          ex_score: number
          gauge: string
          id?: string
          idempotency_key: string
          judge_algorithm: string
          judges: Json
          key_mode: string
          ln_policy: string
          max_combo: number
          min_bp: number
          min_cb: number
          notes: number
          pass_notes: number
          platform: string
          play_mode: string
          play_options?: Json
          played_at?: string | null
          player_id: string
          rejection_reason?: string | null
          replay_format?: string | null
          replay_hash?: string | null
          replay_upload_intent?: string | null
          scoring: string
          server_received_at?: string
          verification?: string
        }
        Update: {
          accepted?: boolean
          avg_judge_ms?: number | null
          bp?: number
          cb?: number
          chart_sha256?: string
          clear_rank?: number
          clear_type?: string
          client_name?: string
          client_version?: string
          created_at?: string
          device_type?: string
          duration_ms?: number | null
          effective_ln_mode?: string
          evidence?: Json
          ex_score?: number
          gauge?: string
          id?: string
          idempotency_key?: string
          judge_algorithm?: string
          judges?: Json
          key_mode?: string
          ln_policy?: string
          max_combo?: number
          min_bp?: number
          min_cb?: number
          notes?: number
          pass_notes?: number
          platform?: string
          play_mode?: string
          play_options?: Json
          played_at?: string | null
          player_id?: string
          rejection_reason?: string | null
          replay_format?: string | null
          replay_hash?: string | null
          replay_upload_intent?: string | null
          scoring?: string
          server_received_at?: string
          verification?: string
        }
        Relationships: [
          {
            foreignKeyName: "scores_chart_sha256_fkey"
            columns: ["chart_sha256"]
            isOneToOne: false
            referencedRelation: "charts"
            referencedColumns: ["sha256"]
          },
          {
            foreignKeyName: "scores_player_id_fkey"
            columns: ["player_id"]
            isOneToOne: false
            referencedRelation: "profiles"
            referencedColumns: ["id"]
          },
        ]
      }
    }
    Views: {
      [_ in never]: never
    }
    Functions: {
      [_ in never]: never
    }
    Enums: {
      [_ in never]: never
    }
    CompositeTypes: {
      [_ in never]: never
    }
  }
}

type DatabaseWithoutInternals = Omit<Database, "__InternalSupabase">

type DefaultSchema = DatabaseWithoutInternals[Extract<keyof Database, "public">]

export type Tables<
  DefaultSchemaTableNameOrOptions extends
    | keyof (DefaultSchema["Tables"] & DefaultSchema["Views"])
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
        DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
      DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])[TableName] extends {
      Row: infer R
    }
    ? R
    : never
  : DefaultSchemaTableNameOrOptions extends keyof (DefaultSchema["Tables"] &
        DefaultSchema["Views"])
    ? (DefaultSchema["Tables"] &
        DefaultSchema["Views"])[DefaultSchemaTableNameOrOptions] extends {
        Row: infer R
      }
      ? R
      : never
    : never

export type TablesInsert<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Insert: infer I
    }
    ? I
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Insert: infer I
      }
      ? I
      : never
    : never

export type TablesUpdate<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Update: infer U
    }
    ? U
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Update: infer U
      }
      ? U
      : never
    : never

export type Enums<
  DefaultSchemaEnumNameOrOptions extends
    | keyof DefaultSchema["Enums"]
    | { schema: keyof DatabaseWithoutInternals },
  EnumName extends DefaultSchemaEnumNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"]
    : never = never,
> = DefaultSchemaEnumNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"][EnumName]
  : DefaultSchemaEnumNameOrOptions extends keyof DefaultSchema["Enums"]
    ? DefaultSchema["Enums"][DefaultSchemaEnumNameOrOptions]
    : never

export type CompositeTypes<
  PublicCompositeTypeNameOrOptions extends
    | keyof DefaultSchema["CompositeTypes"]
    | { schema: keyof DatabaseWithoutInternals },
  CompositeTypeName extends PublicCompositeTypeNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"]
    : never = never,
> = PublicCompositeTypeNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"][CompositeTypeName]
  : PublicCompositeTypeNameOrOptions extends keyof DefaultSchema["CompositeTypes"]
    ? DefaultSchema["CompositeTypes"][PublicCompositeTypeNameOrOptions]
    : never

export const Constants = {
  graphql_public: {
    Enums: {},
  },
  public: {
    Enums: {},
  },
} as const
