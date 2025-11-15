# ホットデプロイガイド

IndigoLispOSは、開発サイクルを高速化するためのホットデプロイ機能を搭載しています。
SDカードの抜き差しなしで、ネットワーク経由でカーネルを更新できます。

## アーキテクチャ

```
開発マシン                     Raspberry Pi 5
    │                              │
    ├─ ファイル監視                │
    │  (inotify/polling)           │
    │                              │
    ├─ 自動ビルド                  │
    │  make                        │
    │                              │
    ├─ UDP転送            UDP:8888 │
    │  hotdeploy_send.py  ─────────>│─ HotDeployReceiver
    │                              │
    │                         ┌────┴────┐
    │                         │ Verify  │
    │                         │ & Load  │
    │                         └────┬────┘
    │                              │
    │                         ┌────┴────┐
    │                         │ kexec   │
    │                         │新カーネル│
    │                         │起動     │
    │                         └─────────┘
```

## 機能レベル

### レベル1: Lispコードのホットリロード ✓

REPLで関数を再定義するだけで即座に反映されます。

```lisp
; 関数を定義
> (define factorial (lambda (n)
    (if (<= n 1) 1
        (* n (factorial (- n 1))))))

; 実行
> (factorial 5)
120

; 関数を再定義（最適化版）
> (define factorial (lambda (n)
    (letrec ((fact-iter (lambda (n acc)
                          (if (<= n 1) acc
                              (fact-iter (- n 1) (* n acc))))))
      (fact-iter n 1))))

; 即座に新しい実装が使われる
> (factorial 5)
120
```

**特徴:**
- 関数の動的再定義
- 履歴管理（`HotReloadManager`）
- ロールバック機能

### レベル2: ネットワークスタック ✓

Ethernetドライバとsmoltcpを統合し、TCP/UDP通信が可能です。

**設定:**
- IP: 192.168.10.110
- Gateway: 192.168.10.1
- MAC: 02:00:00:00:00:01

**実装:**
- `drivers/ethernet.rs` - Ethernetドライバ
- `src-rust/src/network.rs` - smoltcp統合
- UDP/TCPソケット対応

### レベル3: ホットデプロイレシーバー ✓

UDPポート8888でカーネルイメージを受信します。

**プロトコル:**
```
┌─────────────────────────────┐
│ KernelHeader (24 bytes)     │
├─────────────────────────────┤
│ magic:     0x494C4F53       │
│ version:   1                │
│ size:      <kernel size>    │
│ checksum:  <CRC32>          │
│ timestamp: <Unix time>      │
└─────────────────────────────┘
┌─────────────────────────────┐
│ Kernel Image Data           │
│ (chunked UDP packets)       │
└─────────────────────────────┘
```

**実装:**
- `src-rust/src/hotdeploy.rs` - レシーバー＆kexec

### レベル4: カーネルホットスワップ ✓

新しいカーネルイメージを受信後、kexec方式で起動します。

**処理フロー:**
1. カーネルイメージ受信
2. チェックサム検証
3. メモリ（0x80000）にロード
4. データキャッシュフラッシュ
5. 命令キャッシュ無効化
6. 新カーネルへジャンプ

**制約:**
- 状態は保存されません（完全リブート）
- ヒープ、スタック、REPLの状態はリセット
- ハードウェアレジスタは一部保持される可能性

## 使用方法

### セットアップ

1. **ネットワーク接続**
   ```bash
   # Raspberry Pi 5をEthernetで192.168.10.xネットワークに接続
   # または、network.rsでIP設定を変更
   ```

2. **カーネルビルド**
   ```bash
   cd /path/to/IndigoLispOS
   make
   ```

### ワンタイムデプロイ

```bash
# カーネルを手動でホットデプロイ
make hotdeploy

# または異なるIPを指定
make hotdeploy RPI_IP=192.168.10.150
```

### 自動監視モード（推奨）

```bash
# ファイル変更を監視し、自動ビルド＆デプロイ
make watch-hotdeploy
```

**動作:**
- `src-rust/`, `src-c/`, `boot/`, `drivers/`, `lisp/` を監視
- ファイル変更時に自動ビルド
- ビルド成功時に自動デプロイ
- Raspberry Pi側で自動リロード

### 手動スクリプト実行

```bash
# カーネルイメージを直接送信
./tools/hotdeploy_send.py kernel8.img 192.168.10.110 8888

# 継続的監視
./tools/watch_hotdeploy.sh
```

## 開発ワークフロー

### 理想的なセットアップ

**ターミナル1: ビルド＆デプロイ監視**
```bash
make watch-hotdeploy
```

**ターミナル2: シリアルコンソール**
```bash
# Raspberry Piとシリアル接続
screen /dev/ttyUSB0 115200
# または
cu -l /dev/ttyUSB0 -s 115200
```

**ターミナル3: コード編集**
```bash
vim src-rust/src/lib.rs
# または好きなエディタ
```

### 典型的な開発サイクル

1. コードを編集（例: `lisp/eval.rs`に新機能追加）
2. 保存（自動的にビルド開始）
3. ビルド完了（数秒）
4. 自動デプロイ（UDP転送、1-2秒）
5. Raspberry Pi自動リブート
6. シリアルコンソールで新機能をREPLでテスト

**所要時間:** 変更から動作確認まで約10-20秒

## トラブルシューティング

### ホットデプロイが失敗する

**原因1: ネットワーク未接続**
```bash
# Raspberry Pi側でEthernet接続を確認
# シリアルコンソールに以下が表示されているか確認:
# "Ethernet initialized"
# "Network stack ready (192.168.10.110)"
# "Hot deploy ready on port 8888"
```

**原因2: ファイアウォール**
```bash
# 開発マシンからpingテスト
ping 192.168.10.110

# UDPポート8888が開いているか確認
nc -u -v 192.168.10.110 8888
```

**原因3: IPアドレス不一致**
```rust
// src-rust/src/lib.rs の kernel_init() で確認
let ip = Ipv4Address::new(192, 168, 10, 110);  // ← この値
```

### inotify-toolsがない

```bash
# Ubuntu/Debian
sudo apt install inotify-tools

# または、ポーリングモードに自動フォールバック
# （watch_hotdeploy.shが自動判定）
```

### カーネルサイズが大きすぎる

```toml
# src-rust/Cargo.toml で最適化
[profile.release]
opt-level = "z"  # サイズ最適化
lto = true       # Link Time Optimization
```

## パフォーマンス

### 転送速度

- **小規模カーネル** (100KB): ~0.5秒
- **中規模カーネル** (500KB): ~2秒
- **大規模カーネル** (2MB): ~8秒

### 開発サイクル

従来（SDカード方式）:
1. ビルド: 5-10秒
2. SDカード抜く: 5秒
3. PCに挿入: 5秒
4. ファイルコピー: 2秒
5. アンマウント: 2秒
6. SDカード戻す: 5秒
7. 再起動: 5秒
**合計: 29-34秒**

ホットデプロイ:
1. ビルド: 5-10秒
2. ネットワーク転送: 0.5-2秒
3. 自動リブート: 2秒
**合計: 7.5-14秒**

**効率化: 約3倍高速**

## セキュリティ注意

⚠️ **本番環境では使用しないでください**

- 認証なし
- 暗号化なし
- 署名検証なし

これは**開発専用**の機能です。

## 今後の拡張

### 計画中の機能

1. **状態保存**
   - REPLの履歴保持
   - グローバル変数の永続化
   - スケジューラタスクの移行

2. **差分アップデート**
   - 全体ではなく変更部分のみ転送
   - bsdiff/xdelta3による差分生成

3. **認証・暗号化**
   - TLS over TCP
   - 署名検証
   - ロールバック保護

4. **デバッグ統合**
   - GDBスタブとの連携
   - リモートデバッグ
   - ライブメモリ検査

5. **マルチターゲット**
   - 複数のRaspberry Piに同時デプロイ
   - クラスタ一括更新

## 関連ファイル

```
IndigoLispOS/
├── tools/
│   ├── hotdeploy_send.py     # カーネル送信スクリプト
│   └── watch_hotdeploy.sh    # ファイル監視＆自動デプロイ
├── drivers/
│   └── ethernet.rs           # Ethernetドライバ
├── src-rust/src/
│   ├── network.rs            # ネットワークスタック（smoltcp）
│   ├── hotdeploy.rs          # ホットデプロイレシーバー＆kexec
│   └── lib.rs                # メインループ（ポーリング統合）
├── lisp/
│   └── hotreload.rs          # Lispホットリロード管理
└── Makefile                  # ビルドターゲット追加
```

## まとめ

ホットデプロイ機能により、IndigoLispOSの開発サイクルが大幅に高速化されます：

✓ **レベル1**: Lisp関数の動的再定義  
✓ **レベル2**: ネットワークスタック統合  
✓ **レベル3**: カーネルイメージ受信  
✓ **レベル4**: kexecによるホットスワップ  

開発時は`make watch-hotdeploy`を実行して、快適な開発体験をお楽しみください！
