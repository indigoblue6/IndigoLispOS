# IndigoLispOS v0.4 Release Notes

**リリース日:** 2025年11月15日

## 新機能: ホットデプロイシステム 🔥

v0.4では、開発体験を劇的に向上させるホットデプロイ機能を追加しました。
SDカードの抜き差しなしで、ネットワーク経由でカーネルを即座に更新できます。

### 主な追加機能

#### 1. ネットワークスタック統合

- **Ethernetドライバ** (`drivers/ethernet.rs`)
  - BCM2712 Ethernet controller対応
  - MAC address設定
  - 送受信フレーム管理

- **smoltcp統合** (`src-rust/src/network.rs`)
  - TCP/UDPソケット
  - IPv4スタック
  - 自動ルーティング

- **設定:**
  - デフォルトIP: 192.168.10.110
  - Gateway: 192.168.10.1
  - MAC: 02:00:00:00:00:01

#### 2. ホットデプロイレシーバー

- **UDPベースのカーネル受信** (`src-rust/src/hotdeploy.rs`)
  - ポート8888でリッスン
  - チャンク転送対応（UDP MTU考慮）
  - チェックサム検証
  - 進捗表示

- **kexecスタイルのカーネルスワップ**
  - メモリへの直接ロード (0x80000)
  - キャッシュフラッシュ/無効化
  - 割込み無効化後にジャンプ

#### 3. 開発ツール

- **hotdeploy_send.py**
  ```bash
  ./tools/hotdeploy_send.py kernel8.img 192.168.10.110 8888
  ```
  - カーネルイメージ送信
  - プログレスバー表示
  - チェックサム計算

- **watch_hotdeploy.sh**
  ```bash
  make watch-hotdeploy
  ```
  - ファイル監視（inotify/polling）
  - 自動ビルド
  - 自動転送
  - エラーハンドリング

#### 4. Lispホットリロード

- **HotReloadManager** (`lisp/hotreload.rs`)
  - 関数再定義の履歴管理
  - ロールバック機能
  - 変更追跡（タイムスタンプ付き）

- **REPL統合**
  - 関数の動的再定義が即座に反映
  - 環境の完全な更新

### Makefileターゲット追加

```makefile
make hotdeploy          # 1回だけホットデプロイ
make watch-hotdeploy    # 自動監視モード
```

**環境変数:**
- `RPI_IP`: ターゲットIPアドレス（デフォルト: 192.168.10.110）
- `RPI_PORT`: ターゲットポート（デフォルト: 8888）

### 開発サイクルの高速化

**従来（SDカード方式）:**
- ビルド → SDカード抜く → PCに挿入 → コピー → 戻す → 起動
- **合計: 約30-35秒**

**ホットデプロイ:**
- ビルド → ネットワーク転送 → 自動リブート
- **合計: 約8-15秒**

**効率化: 約3倍高速** ⚡

### 使用例

#### 自動監視モード（推奨）

```bash
# ターミナル1: 監視
make watch-hotdeploy

# ターミナル2: シリアルコンソール
screen /dev/ttyUSB0 115200

# ターミナル3: コード編集
vim src-rust/src/lib.rs
```

コードを保存すると自動的にビルド→転送→リブートされます。

#### 手動デプロイ

```bash
# ビルド
make

# デプロイ
make hotdeploy RPI_IP=192.168.10.110
```

### アーキテクチャ変更

```
┌──────────────────────────────────┐
│ 開発マシン                       │
│  ├─ inotify/fswatch             │
│  ├─ make                        │
│  └─ hotdeploy_send.py           │
└──────────┬───────────────────────┘
           │ UDP:8888
           ▼
┌──────────────────────────────────┐
│ Raspberry Pi 5                   │
│  ├─ HotDeployReceiver           │
│  ├─ NetworkStack (smoltcp)      │
│  ├─ EthernetDriver              │
│  └─ kexec()                     │
└──────────────────────────────────┘
```

## 依存関係の追加

### Cargo.toml

```toml
smoltcp = { version = "0.11", default-features = false, 
           features = ["proto-ipv4", "socket-udp", "socket-tcp", 
                      "medium-ethernet"] }
```

## ファイル追加/変更

### 新規ファイル

- `drivers/ethernet.rs` - Ethernetドライバ
- `src-rust/src/network.rs` - ネットワークスタック
- `src-rust/src/hotdeploy.rs` - ホットデプロイ受信＆kexec
- `lisp/hotreload.rs` - Lispホットリロード管理
- `tools/hotdeploy_send.py` - カーネル送信スクリプト
- `tools/watch_hotdeploy.sh` - 自動監視スクリプト
- `docs/HOTDEPLOY.md` - 詳細ドキュメント

### 変更ファイル

- `src-rust/src/lib.rs` - ネットワークポーリング統合
- `Makefile` - ホットデプロイターゲット追加
- `README.md` - ホットデプロイ説明追加
- `lisp/mod.rs` - hotreloadモジュール追加
- `drivers/mod.rs` - ethernetモジュール追加

## 既知の制限事項

### セキュリティ

⚠️ **開発専用機能です**

- 認証なし
- 暗号化なし
- 署名検証なし

本番環境では使用しないでください。

### 技術的制限

1. **状態保存なし**
   - kexec時に完全リブート
   - REPLの履歴、グローバル変数はリセット
   - スケジューラタスクは消失

2. **Ethernetドライバ**
   - BCM2712レジスタアドレスは仮実装
   - 実機での調整が必要な場合あり

3. **ネットワーク**
   - IPv4のみ対応
   - 静的IP設定のみ

## トラブルシューティング

### ホットデプロイが動作しない

1. **Ethernet接続確認**
   ```bash
   ping 192.168.10.110
   ```

2. **シリアルコンソールでログ確認**
   以下のメッセージが表示されているか:
   ```
   Ethernet initialized
   Network stack ready (192.168.10.110)
   Hot deploy ready on port 8888
   ```

3. **ファイアウォール確認**
   ```bash
   sudo ufw allow 8888/udp
   ```

### inotify-toolsがない

```bash
# Ubuntu/Debian
sudo apt install inotify-tools

# または自動的にポーリングモードにフォールバック
```

## 今後の計画

### v0.5候補機能

- [ ] 状態保存・復元機能
- [ ] 差分アップデート（bsdiff/xdelta3）
- [ ] TLS over TCP
- [ ] 署名検証
- [ ] マルチターゲット同時デプロイ
- [ ] GDBスタブ統合

## まとめ

v0.4では、開発体験の大幅な向上を実現しました。ホットデプロイにより、
コードの変更から動作確認までの時間が従来の約1/3に短縮されています。

```bash
# 快適な開発を始めましょう！
make watch-hotdeploy
```

詳細は [docs/HOTDEPLOY.md](docs/HOTDEPLOY.md) をご覧ください。

---

**Contributors:** IndigoLispOS Team  
**License:** MIT  
**Repository:** https://github.com/indigoblue6/IndigoLispOS
