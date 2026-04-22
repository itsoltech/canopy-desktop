import { app, net } from 'electron'
import { mkdir, readFile, writeFile } from 'fs/promises'
import { dirname, join } from 'path'
import type { ClaudeProviderPresetId } from '../../shared/claudeProviderPresets'

interface ParsedModel {
  id: string
  name: string
  providerId: string | null
  status: string[]
  family: string | null
  releaseDate: string | null
  lastUpdated: string | null
}

export interface ModelOption {
  value: string
  label: string
  family?: string | null
  releaseDate?: string | null
  lastUpdated?: string | null
}

interface CachedCatalogFile {
  fetchedAt: string
  source: string
  payload: unknown
}

const MODELS_DEV_URL = 'https://models.dev/api.json'
const CACHE_TTL_MS = 24 * 60 * 60 * 1000

const PROVIDER_ALIASES: Record<ClaudeProviderPresetId, string[]> = {
  anthropic: ['anthropic'],
  kimi: ['moonshot', 'kimi'],
  minimax: ['minimax'],
  zai: ['z-ai', 'zai', 'zhipu'],
}

function normalize(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

function statusFromNode(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.filter((item): item is string => typeof item === 'string')
  }
  if (typeof value === 'string') return [value]
  return []
}

function parseCatalog(payload: unknown): ParsedModel[] {
  if (!payload || typeof payload !== 'object' || Array.isArray(payload)) return []

  const providers = payload as Record<string, unknown>
  const models: ParsedModel[] = []

  for (const [providerKey, providerValue] of Object.entries(providers)) {
    if (!providerValue || typeof providerValue !== 'object' || Array.isArray(providerValue))
      continue
    const providerRecord = providerValue as Record<string, unknown>
    const providerId =
      typeof providerRecord.id === 'string' ? normalize(providerRecord.id) : normalize(providerKey)
    const providerModels = providerRecord.models
    if (!providerModels || typeof providerModels !== 'object' || Array.isArray(providerModels)) {
      continue
    }

    for (const [modelKey, modelValue] of Object.entries(providerModels)) {
      if (!modelValue || typeof modelValue !== 'object' || Array.isArray(modelValue)) continue
      const modelRecord = modelValue as Record<string, unknown>
      const id = typeof modelRecord.id === 'string' ? modelRecord.id : modelKey
      const name = typeof modelRecord.name === 'string' ? modelRecord.name : id
      models.push({
        id,
        name,
        providerId,
        status: statusFromNode(modelRecord.status),
        family: typeof modelRecord.family === 'string' ? normalize(modelRecord.family) : null,
        releaseDate: typeof modelRecord.release_date === 'string' ? modelRecord.release_date : null,
        lastUpdated: typeof modelRecord.last_updated === 'string' ? modelRecord.last_updated : null,
      })
    }
  }

  return models
}

function matchesPreset(model: ParsedModel, preset: ClaudeProviderPresetId): boolean {
  const aliases = PROVIDER_ALIASES[preset]
  const providerId = normalize(model.providerId)
  const modelId = normalize(model.id)
  const name = normalize(model.name)

  if (aliases.includes(providerId)) return true

  if (preset === 'zai') return modelId.includes('glm') || name.includes('glm')
  if (preset === 'kimi') return modelId.includes('kimi') || name.includes('kimi')

  return false
}

function compareModels(a: ParsedModel, b: ParsedModel): number {
  const aDate = a.releaseDate ?? a.lastUpdated ?? ''
  const bDate = b.releaseDate ?? b.lastUpdated ?? ''
  if (aDate !== bDate) return bDate.localeCompare(aDate)
  return a.name.localeCompare(b.name)
}

function stripLatestSuffix(name: string): string {
  return name.replace(/\s+\(latest\)$/i, '')
}

export class ModelsDevCatalog {
  private inflightRefresh: Promise<unknown> | null = null

  private get cachePath(): string {
    return join(app.getPath('userData'), 'models.dev.json')
  }

  async getOptionsForPreset(preset: ClaudeProviderPresetId): Promise<ModelOption[]> {
    const payload = await this.loadPayload()
    const models = parseCatalog(payload)

    // Dedupe within the preset's matched subset. Doing this before the
    // preset filter would let other providers (e.g. amazon-bedrock, 302ai)
    // claim shared model IDs first and squeeze the real provider out.
    const matched = models
      .filter((model) => !model.status.includes('deprecated'))
      .filter((model) => matchesPreset(model, preset))

    const deduped = new Map<string, ParsedModel>()
    for (const model of matched) {
      if (!deduped.has(model.id)) deduped.set(model.id, model)
    }

    let filtered = [...deduped.values()].sort(compareModels)

    if (preset === 'anthropic') {
      const concreteNames = new Set(
        filtered.filter((model) => !/\(latest\)$/i.test(model.name)).map((model) => model.name),
      )
      filtered = filtered.filter((model) => {
        if (!/\(latest\)$/i.test(model.name)) return true
        return !concreteNames.has(stripLatestSuffix(model.name))
      })
    }

    const nameCounts = new Map<string, number>()
    for (const model of filtered) {
      nameCounts.set(model.name, (nameCounts.get(model.name) ?? 0) + 1)
    }

    return filtered.map((model) => ({
      value: model.id.includes('/') ? model.id.split('/').slice(1).join('/') : model.id,
      label: (nameCounts.get(model.name) ?? 0) > 1 ? `${model.name} (${model.id})` : model.name,
      family: model.family,
      releaseDate: model.releaseDate,
      lastUpdated: model.lastUpdated,
    }))
  }

  private async loadPayload(): Promise<unknown> {
    const cached = await this.readCache()
    const isFresh =
      cached !== null && Date.now() - new Date(cached.fetchedAt).getTime() < CACHE_TTL_MS

    if (isFresh) return cached.payload

    try {
      return await this.refreshCache()
    } catch (error) {
      if (cached) return cached.payload
      throw error
    }
  }

  private async readCache(): Promise<CachedCatalogFile | null> {
    try {
      const raw = await readFile(this.cachePath, 'utf8')
      const parsed = JSON.parse(raw) as CachedCatalogFile
      if (!parsed || typeof parsed !== 'object') return null
      if (typeof parsed.fetchedAt !== 'string') return null
      return parsed
    } catch {
      return null
    }
  }

  private async refreshCache(): Promise<unknown> {
    if (!this.inflightRefresh) {
      this.inflightRefresh = this.fetchPayload()
        .then(async (payload) => {
          const cacheRecord: CachedCatalogFile = {
            fetchedAt: new Date().toISOString(),
            source: MODELS_DEV_URL,
            payload,
          }
          await mkdir(dirname(this.cachePath), { recursive: true })
          await writeFile(this.cachePath, JSON.stringify(cacheRecord), 'utf8')
          return payload
        })
        .finally(() => {
          this.inflightRefresh = null
        })
    }

    return this.inflightRefresh
  }

  private async fetchPayload(): Promise<unknown> {
    const response = await net.fetch(MODELS_DEV_URL, {
      bypassCustomProtocolHandlers: true,
    })
    if (!response.ok) {
      throw new Error(`models.dev returned ${response.status}`)
    }
    return response.json()
  }
}
