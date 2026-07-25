import type { SearchResult } from '../ingest/document-store.js'
import type { ModelPlugin } from '../plugin/interfaces.js'
import type { QueryIntent } from './intent.js'

/**
 * Intent-specific weight profiles for fallback reranking.
 * Weights: [originalScore, wordMatch, ngramPhrase, headingBoost, chunkTypeBoost]
 */
const INTENT_WEIGHTS: Record<string, number[]> = {
  code:    [0.3, 0.2, 0.1, 0.15, 0.25],
  concept: [0.35, 0.3, 0.15, 0.15, 0.05],
  config:  [0.35, 0.25, 0.15, 0.15, 0.1],
  data:    [0.3, 0.2, 0.1, 0.15, 0.25],
  search:  [0.4, 0.25, 0.15, 0.2, 0.0],
  compare: [0.35, 0.3, 0.15, 0.15, 0.05],
  general: [0.4, 0.25, 0.15, 0.2, 0.0],
}

const INTENT_CHUNK_PREFERENCES: Record<string, string[]> = {
  code: ['code-ast'],
  config: ['code-ast', 'semantic'],
  data: ['table'],
  concept: ['semantic'],
}

/**
 * Rerank search results using external API (Cohere, Jina) or model's rerank capability,
 * with fall back to improved keyword scoring with heading boost, partial matching,
 * and intent-adaptive weight profiles. Includes high-precision Score Filtering.
 */
export async function rerankResults(
  query: string,
  results: SearchResult[],
  rerankConfig?: {
    rerankerProvider?: string;
    rerankerApiKey?: string;
    rerankerBaseUrl?: string;
    rerankerScoreThreshold?: number;
  },
  intent?: QueryIntent
): Promise<SearchResult[]> {
  if (results.length <= 1) return results

  const provider = rerankConfig?.rerankerProvider || 'local'
  const apiKey = rerankConfig?.rerankerApiKey || ''
  const baseUrl = rerankConfig?.rerankerBaseUrl || ''
  const threshold = rerankConfig?.rerankerScoreThreshold ?? 0.6

  let scoredResults: SearchResult[] = []

  // 1. Cohere Reranker API Integration (perfect for multilingual and office files)
  if (provider === 'cohere' && apiKey) {
    try {
      const cohereUrl = baseUrl || 'https://api.cohere.ai/v1/rerank'
      const response = await fetch(cohereUrl, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          model: 'rerank-multilingual-v3.0',
          query: query,
          documents: results.map(r => r.content),
          top_n: results.length,
        }),
      })

      if (!response.ok) {
        throw new Error(`Cohere API error: ${response.status} ${response.statusText}`)
      }

      const data = await response.json() as any
      if (data && Array.isArray(data.results)) {
        scoredResults = data.results.map((res: any) => ({
          ...results[res.index],
          score: res.relevance_score,
        }))
      }
    } catch (err) {
      console.warn('[reranker] Cohere rerank failed, falling back to local heuristic:', err instanceof Error ? err.message : String(err))
    }
  }
  // 2. Jina Reranker API Integration (great fallback option)
  else if (provider === 'jina' && apiKey) {
    try {
      const jinaUrl = baseUrl || 'https://api.jina.ai/v1/rerank'
      const response = await fetch(jinaUrl, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          model: 'jina-reranker-v2-base-multilingual',
          query: query,
          documents: results.map(r => r.content),
          top_n: results.length,
        }),
      })

      if (!response.ok) {
        throw new Error(`Jina API error: ${response.status} ${response.statusText}`)
      }

      const data = await response.json() as any
      if (data && Array.isArray(data.results)) {
        scoredResults = data.results.map((res: any) => ({
          ...results[res.index],
          score: res.relevance_score,
        }))
      }
    } catch (err) {
      console.warn('[reranker] Jina rerank failed, falling back to local heuristic:', err instanceof Error ? err.message : String(err))
    }
  }

  // 3. Fallback to Local Heuristic if no external model is used or if it failed
  if (scoredResults.length === 0) {
    scoredResults = await runFallbackLocalRerank(query, results, intent)
  }

  // 4. Score Filter (Fuse Filter):
  // Filter out noisy documents with low confidence.
  // External APIs (Cohere/Jina) output a stable 0.0 ~ 1.0 probability score.
  const isExternalModelUsed = scoredResults.length > 0 && scoredResults[0] !== undefined && (provider === 'cohere' || provider === 'jina')
  let finalResults = scoredResults

  if (isExternalModelUsed) {
    finalResults = scoredResults.filter(r => r.score >= threshold)

    // Self-healing safety guard: if threshold is too strict and filters everything out,
    // fallback to keeping the best matched candidate to avoid blank results.
    if (finalResults.length === 0 && results.length > 0 && threshold < 0.8) {
      finalResults = [scoredResults[0]]
    }
  }

  return finalResults.sort((a, b) => b.score - a.score)
}

/**
 * High-quality fallback local keyword word-boundary, n-gram phrase scoring,
 * and heading boost adapter.
 */
export async function runFallbackLocalRerank(
  query: string,
  results: SearchResult[],
  intent?: QueryIntent
): Promise<SearchResult[]> {
  if (results.length <= 1) return results

  // Improved fallback: word-boundary matching + n-gram phrase scoring + heading boost
  const queryWords = query.toLowerCase().split(/\s+/).filter(w => w.length > 1)

  return results
    .map(r => {
      const contentLower = r.content.toLowerCase()
      const headingText = (r.headingHierarchy || []).join(' ').toLowerCase()

      // Word-boundary matching: prevent false positives like "auth" matching "author"
      let contentMatches = 0
      for (const qw of queryWords) {
        if (matchesWordBoundary(qw, contentLower)) contentMatches++
      }
      const wordScore = queryWords.length > 0 ? contentMatches / queryWords.length : 0

      // N-gram phrase bonus: consecutive query word pairs/triples appearing together
      const ngramScore = computeNgramScore(queryWords, contentLower)

      // Heading boost: query words in headings are strong relevance signals
      let headingMatches = 0
      for (const qw of queryWords) {
        if (matchesWordBoundary(qw, headingText)) headingMatches++
      }
      const headingScore = queryWords.length > 0 ? headingMatches / queryWords.length : 0

      // Chunk type alignment bonus
      const preferredTypes = intent ? INTENT_CHUNK_PREFERENCES[intent] : undefined
      const chunkTypeBonus = preferredTypes && preferredTypes.includes(r.chunkType) ? 1.0 : 0.0

      // Intent-adaptive weights: [original, word, ngram, heading, chunkType]
      const w = INTENT_WEIGHTS[intent || 'general']
      const finalScore = r.score * w[0] + wordScore * w[1] + ngramScore * w[2] + headingScore * w[3] + chunkTypeBonus * w[4]

      return { ...r, score: finalScore }
    })
    .sort((a, b) => b.score - a.score)
}

/** Escape special regex characters in a string. */
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** Check if a query word appears as a whole word in the text (word-boundary matching). */
function matchesWordBoundary(word: string, text: string): boolean {
  const boundary = '[\\s.,;:!?()\\[\\]{}"\'\`/\\-]'
  const pattern = new RegExp(`(?:^|${boundary})${escapeRegExp(word)}(?:$|${boundary})`, 'i')
  return pattern.test(` ${text} `)
}

/**
 * Compute n-gram phrase score for consecutive query word pairs and triples.
 * Returns a value between 0 and 1 indicating how many consecutive n-grams appear in the content.
 */
function computeNgramScore(queryWords: string[], content: string): number {
  if (queryWords.length < 2) return 0

  let matchCount = 0
  let totalNgrams = 0

  // Bigrams (consecutive pairs)
  for (let i = 0; i < queryWords.length - 1; i++) {
    totalNgrams++
    const bigram = `${queryWords[i]} ${queryWords[i + 1]}`
    if (content.includes(bigram)) matchCount++
  }

  // Trigrams (consecutive triples)
  for (let i = 0; i < queryWords.length - 2; i++) {
    totalNgrams++
    const trigram = `${queryWords[i]} ${queryWords[i + 1]} ${queryWords[i + 2]}`
    if (content.includes(trigram)) matchCount++
  }

  return totalNgrams > 0 ? matchCount / totalNgrams : 0
}
