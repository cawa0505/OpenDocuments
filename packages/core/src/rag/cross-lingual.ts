export type CustomDictionary = Record<string, string>

let customDict: CustomDictionary = {}
let customReverseDict: CustomDictionary = {}

/**
 * Dynamically load a custom cross-lingual dictionary for query expansion.
 */
export function loadCustomDictionary(dict: CustomDictionary) {
  customDict = { ...dict }
  customReverseDict = {}
  for (const [key, val] of Object.entries(dict)) {
    customReverseDict[val.toLowerCase()] = key
  }
}

/**
 * Check if the text contains non-English characters (Chinese, Japanese, Korean, etc.).
 */
export function containsNonEnglish(text: string): boolean {
  return /[\u4e00-\u9fa5\uac00-\ud7af]/.test(text)
}

/**
 * Expand search query based on custom cross-lingual dictionary alignment.
 * If query is non-English, add translated English keywords from custom dictionary.
 * If query is English, add original non-English terms from reverse dictionary.
 */
export function expandQuery(query: string): string[] {
  const queries = [query]
  const lower = query.toLowerCase()

  if (containsNonEnglish(query)) {
    // Non-English query: add English translations from dictionary
    const translations: string[] = []
    for (const [nonEn, en] of Object.entries(customDict)) {
      if (lower.includes(nonEn.toLowerCase())) {
        translations.push(en)
      }
    }
    if (translations.length > 0) {
      queries.push(translations.join(' '))
    }
  } else {
    // English query: add non-English keyword translations from reverse dictionary
    const translations: string[] = []
    for (const [en, nonEn] of Object.entries(customReverseDict)) {
      if (lower.includes(en)) {
        translations.push(nonEn)
      }
    }
    if (translations.length > 0) {
      queries.push(translations.join(' '))
    }
  }

  return queries
}

/**
 * Merge results from multiple query variants using Reciprocal Rank Fusion.
 */
export function reciprocalRankFusion<T extends { score: number }>(
  resultSets: T[][],
  k = 60,
  getKey?: (item: T) => string,
  scoreWeighted = false
): T[] {
  const scores = new Map<string, { item: T; score: number }>()

  for (const results of resultSets) {
    for (let rank = 0; rank < results.length; rank++) {
      const item = results[rank]
      // Key excludes score so items with same content but different scores are deduped
      const { score: _score, ...rest } = item as T & { score: number }
      const key = getKey ? getKey(item) : JSON.stringify(rest)
      const existing = scores.get(key)
      const rrfBase = 1 / (k + rank + 1)
      const rrfScore = scoreWeighted ? rrfBase * item.score : rrfBase

      if (existing) {
        existing.score += rrfScore
      } else {
        scores.set(key, { item, score: rrfScore })
      }
    }
  }

  return Array.from(scores.values())
    .sort((a, b) => b.score - a.score)
    .map(({ item, score }) => ({ ...item, score }))
}
