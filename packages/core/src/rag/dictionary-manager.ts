import { randomUUID } from 'node:crypto'
import type { DB } from '../storage/db.js'
import { loadCustomDictionary } from './cross-lingual.js'

export interface DictionaryEntry {
  id: string
  workspaceId: string
  key: string
  value: string
  createdAt: string
}

export class DictionaryManager {
  constructor(private db: DB, private workspaceId: string) {}

  /**
   * Add a new key-value pair to the workspace dictionary or update if key exists.
   */
  set(key: string, value: string): DictionaryEntry {
    const trimmedKey = key.trim()
    const trimmedValue = value.trim()
    
    // Check if key already exists
    const existing = this.db.get<any>(
      'SELECT id, created_at FROM dictionary WHERE workspace_id = ? AND key = ?',
      [this.workspaceId, trimmedKey]
    )

    const now = new Date().toISOString()
    if (existing) {
      this.db.run(
        'UPDATE dictionary SET value = ?, created_at = ? WHERE id = ?',
        [trimmedValue, now, existing.id]
      )
      return {
        id: existing.id,
        workspaceId: this.workspaceId,
        key: trimmedKey,
        value: trimmedValue,
        createdAt: existing.created_at,
      }
    } else {
      const id = randomUUID()
      this.db.run(
        'INSERT INTO dictionary (id, workspace_id, key, value, created_at) VALUES (?, ?, ?, ?, ?)',
        [id, this.workspaceId, trimmedKey, trimmedValue, now]
      )
      return {
        id,
        workspaceId: this.workspaceId,
        key: trimmedKey,
        value: trimmedValue,
        createdAt: now,
      }
    }
  }

  /**
   * Alias for set() for UPSERT actions.
   */
  upsert(key: string, value: string): DictionaryEntry {
    return this.set(key, value)
  }

  /**
   * Optional Seeds: Import a dictionary seed dataset (traditional chinese or korean technical glossaries).
   * Seeds are executed safely in the context of the current workspaceId.
   */
  importSeed(language: 'zh-TW' | 'ko-KR'): void {
    const glossary: Record<string, string> = language === 'zh-TW' ? {
      '認證': 'authentication', '設定': 'configuration', '部署': 'deployment',
      '安裝': 'installation', '資料庫': 'database', '伺服器': 'server',
      '客戶端': 'client', '使用者': 'user', '管理員': 'admin',
      '安全性': 'security', '權限': 'permission', '登入': 'login',
      '密碼': 'password', '搜尋': 'search', '文檔': 'document',
      '檔案': 'file', '上傳': 'upload', '下載': 'download',
      '錯誤': 'error', '除錯': 'debugging', '修復': 'fix',
      '測試': 'test', '建置': 'build', '執行': 'run',
      '函式': 'function', '變數': 'variable', '型態': 'type',
      '模組': 'module', '套件': 'package', '程式庫': 'library',
      '框架': 'framework', '元件': 'component', '介面': 'interface',
      '環境變數': 'environment variable', '快取': 'cache', '佇列': 'queue',
      '架構': 'architecture', '微服務': 'microservice', '設計': 'design',
      '模式': 'pattern', '依賴': 'dependency', '擴充性': 'scalability',
      '中介軟體': 'middleware', '端點': 'endpoint', '路由': 'routing',
      '閘道': 'gateway', '代理': 'proxy', '負載平衡': 'load balancer',
      '服務網格': 'service mesh', '單體': 'monolithic', '後端': 'backend',
      '前端': 'frontend', '容器': 'container', '管線': 'pipeline',
      '監控': 'monitoring', '基礎設施': 'infrastructure', '雲端': 'cloud',
      '健康檢查': 'health check', '命名空間': 'namespace', '節點': 'node',
      '服務': 'service', '資料卷': 'volume', '機密': 'secret',
      '備份': 'backup', '還原': 'restore', '查詢': 'query',
      '索引': 'index', '交易': 'transaction', '向量': 'vector',
      '嵌入': 'embedding', '相似度': 'similarity', '連線池': 'connection pool',
      '機器學習': 'machine learning', '推論': 'inference', '微調': 'fine-tuning',
      '模型': 'model', '詞記': 'token', '檢索增強生成': 'retrieval augmented generation',
      '切片': 'chunking', '重排': 'reranking'
    } : {
      '인증': 'authentication', '설정': 'configuration', '배포': 'deployment',
      '설치': 'installation', '데이터베이스': 'database', '서버': 'server',
      '클라이언트': 'client', '사용자': 'user', '관리자': 'admin',
      '보안': 'security', '권한': 'permission', '로그인': 'login',
      '비밀번호': 'password', '검색': 'search', '문서': 'document',
      '파일': 'file', '업로드': 'upload', '다운로드': 'download',
      '에러': 'error', '버그': 'bug', '수정': 'fix',
      '테스트': 'test', '빌드': 'build', '실행': 'run',
      '함수': 'function', '변수': 'variable', '타입': 'type',
      '모듈': 'module', '패키지': 'package', '라이브러리': 'library',
      '프레임워크': 'framework', '컴포넌트': 'component', '인터페이스': 'interface',
      '환경변수': 'environment variable', '캐시': 'cache', '큐': 'queue',
      '아키텍처': 'architecture', '마이크로서비스': 'microservice', '설계': 'design',
      '패턴': 'pattern', '의존성': 'dependency', '확장성': 'scalability',
      '미들웨어': 'middleware', '엔드포인트': 'endpoint', '라우팅': 'routing',
      '게이트웨이': 'gateway', '프록시': 'proxy', '로드밸런서': 'load balancer',
      '서비스메시': 'service mesh', '모놀리식': 'monolithic', '백엔드': 'backend',
      '프론트엔드': 'frontend', '컨테이너': 'container', '파이프라인': 'pipeline',
      '모니터링': 'monitoring', '인프라': 'infrastructure', '클라우드': 'cloud',
      '헬스체크': 'health check', '네임스페이스': 'namespace', '노드': 'node',
      '서비스': 'service', '볼륨': 'volume', '시크릿': 'secret',
      '백업': 'backup', '복원': 'restore', '쿼리': 'query',
      '인덱스': 'index', '트랜잭션': 'transaction', '벡터': 'vector',
      '임베딩': 'embedding', '유사도': 'similarity', '연결풀': 'connection pool',
      '머신러닝': 'machine learning', '추론': 'inference', '파인튜닝': 'fine-tuning',
      '모델': 'model', '토큰': 'token', '검색증강생성': 'retrieval augmented generation',
      '청킹': 'chunking', '리랭킹': 'reranking'
    }

    for (const [key, value] of Object.entries(glossary)) {
      this.set(key, value)
    }
  }

  /**
   * List all custom dictionary pairs in the current workspace.
   */
  list(): DictionaryEntry[] {
    return this.db.all<any>(
      'SELECT * FROM dictionary WHERE workspace_id = ? ORDER BY key',
      [this.workspaceId]
    ).map(r => ({
      id: r.id,
      workspaceId: r.workspace_id,
      key: r.key,
      value: r.value,
      createdAt: r.created_at,
    }))
  }

  /**
   * Delete a dictionary entry by key or ID.
   */
  delete(keyOrId: string): void {
    this.db.run(
      'DELETE FROM dictionary WHERE (id = ? OR key = ?) AND workspace_id = ?',
      [keyOrId, keyOrId, this.workspaceId]
    )
  }

  /**
   * Load current workspace's custom glossary and dynamically inject it into the cross-lingual RAG engine.
   */
  loadToEngine(): void {
    const entries = this.list()
    const pairs: Record<string, string> = {}
    for (const entry of entries) {
      pairs[entry.key] = entry.value
    }
    loadCustomDictionary(pairs)
  }
}
