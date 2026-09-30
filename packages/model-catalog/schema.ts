import { z } from 'zod'

export const SHA256_RE = /^[0-9a-f]{64}$/

export const ModelFileSchema = z.object({
  url: z.string().url(),
  sha256: z.string().regex(SHA256_RE, 'sha256 must be 64 lowercase hex characters'),
  archive: z.string().optional(),
})

export const CatalogModelSchema = z.object({
  id: z.string().min(1),
  kind: z.enum(['stt', 'tts']),
  engine: z.string().min(1),
  name: z.string().min(1),
  description: z.string().min(1),
  languages: z.array(z.string()).min(1),
  sizeBytes: z.number().positive(),
  ramRecommendedBytes: z.number().positive(),
  license: z.string().min(1),
  homepage: z.string().url(),
  files: z.array(ModelFileSchema).min(1),
  tags: z.array(z.string()).default([]),
})

export const CatalogSchema = z.object({
  version: z.number().int().positive(),
  models: z.array(CatalogModelSchema),
})

export type ModelFile = z.infer<typeof ModelFileSchema>
export type CatalogModel = z.infer<typeof CatalogModelSchema>
export type Catalog = z.infer<typeof CatalogSchema>