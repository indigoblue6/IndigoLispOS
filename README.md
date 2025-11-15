# IndigoLispOS

**S式がネイティブに動作するOS**

Raspberry Pi 5向けのベアメタルLisp OS実装

## 概要

IndigoLispOSは、Raspberry Pi 5のベアメタル環境上で動作する極小OSです。
起動後、最低限のハードウェア初期化をC言語とアセンブリで行い、Rustランタイムへ制御を移します。

### 特徴

- 🚀 **ミニマリズム** - 余分なレイヤを排除した最小構成
- 🦀 **Rust + C** - 安全性と速度の両立
- 🎨 **S式ネイティブ** - OS API を S式として直接操作
- 🔧 **拡張可能** - Lispらしく動的にOSを拡張
- 📚 **学習性** - 実装全体が理解しやすい構造
- ⚡ **ホットデプロイ** - SDカード不要、ネットワーク経由で即座に更新

## アーキテクチャ

```
┌─────────────────────────────────────┐
│         S式 REPL / Evaluator        │
│         (Rust - no_std)             │
├─────────────────────────────────────┤
│  Hot Deploy Receiver (UDP:8888)     │
│  Network Stack (smoltcp)            │
├─────────────────────────────────────┤
│    Drivers (UART/GPIO/Timer/Net)    │
│         (Rust - no_std)             │
├─────────────────────────────────────┤
│    Kernel Runtime & Allocator       │
│         (Rust - no_std)             │
├─────────────────────────────────────┤
│    Boot & Hardware Init             │
│      (C + AArch64 Assembly)         │
├─────────────────────────────────────┤
│      Raspberry Pi 5 Hardware        │
└─────────────────────────────────────┘
```

## プロジェクト構造

```
IndigoLispOS/
├── boot/              # ブートローダ (Assembly)
│   └── boot.S         # AArch64起動コード
├── src-c/             # C言語カーネル初期化
│   ├── kernel.c       # カーネルエントリポイント
│   ├── uart.c         # UART初期化
│   └── mmu.c          # MMU設定
├── src-rust/          # Rustカーネル本体
│   ├── src/
│   │   ├── lib.rs         # メインエントリ
│   │   ├── allocator.rs   # ヒープアロケータ
│   │   ├── panic.rs       # パニックハンドラ
│   │   ├── interrupt.rs   # 割込み管理 (v0.3)
│   │   ├── scheduler.rs   # タスクスケジューラ (v0.3)
│   │   ├── network.rs     # ネットワークスタック (smoltcp)
│   │   └── hotdeploy.rs   # ホットデプロイ受信＆kexec
│   └── Cargo.toml
├── drivers/           # デバイスドライバ (Rust)
│   ├── uart.rs
│   ├── gpio.rs
│   ├── timer.rs       # 割込み対応 (v0.3)
│   ├── ethernet.rs    # Ethernetドライバ
│   └── mod.rs
├── lisp/              # S式評価器 (Rust)
│   ├── expr.rs        # S式データ型
│   ├── parser.rs      # パーサ
│   ├── env.rs         # 環境・変数管理
│   ├── eval.rs        # 評価器 (spawn, sleep等追加)
│   ├── repl.rs        # REPL (v0.2)
│   ├── hotreload.rs   # ホットリロード管理
│   └── mod.rs
├── tools/             # 開発ツール
│   ├── hotdeploy_send.py    # カーネル送信スクリプト
│   └── watch_hotdeploy.sh   # 自動監視＆デプロイ
├── arch/aarch64/      # アーキテクチャ依存
│   ├── linker.ld      # リンカスクリプト
│   └── interrupts.S   # 割込みベクタテーブル (v0.3)
└── Makefile           # ビルドシステム
```

## ビルド

### 必要なツール

```bash
# AArch64クロスコンパイラ
sudo apt install gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu

# Rustツールチェーン
rustup target add aarch64-unknown-none
```

### ビルド手順

```bash
# カーネルイメージをビルド
make

# または個別に
make rust          # Rustコンポーネントのみ
make clean         # クリーンアップ
```

ビルドが成功すると `kernel8.img` が生成されます。

## 実行

### 実機で実行（従来のSDカード方式）

1. SDカードのFATパーティションに `kernel8.img` をコピー
2. Raspberry Pi 5に挿入して起動

```bash
# SD_MOUNT環境変数を設定して自動デプロイ
make deploy SD_MOUNT=/media/user/boot
```

### ホットデプロイ（推奨・開発時）

ネットワーク経由でカーネルを即座に更新できます。SDカードの抜き差しは不要です。

```bash
# 1回だけデプロイ
make hotdeploy RPI_IP=192.168.10.110

# ファイル監視モード（自動ビルド＆デプロイ）
make watch-hotdeploy
```

**開発サイクル:**
1. コードを編集・保存
2. 自動ビルド（5-10秒）
3. 自動転送（0.5-2秒）
4. Raspberry Pi自動リブート
5. 新しいコードで即座に動作

詳細は [docs/HOTDEPLOY.md](docs/HOTDEPLOY.md) を参照してください。

## REPL使用例

起動後、UARTコンソールでS式を評価できます：

### v0.2の例（Lambda、クロージャ）

```lisp
> (define factorial (lambda (n) (if (< n 2) 1 (* n (factorial (- n 1))))))
<lambda>

> (factorial 5)
120

> (define make-adder (lambda (n) (lambda (x) (+ x n))))
<lambda>

> (define add5 (make-adder 5))
<lambda>

> (add5 10)
15

; Note: Advanced macros require quasiquote/unquote (planned for v0.3)
```

### v0.1の例

```lisp
> (+ 1 2 3)
6

> (- 10 3)
7

> (* 4 5)
20

> (if (> 5 3) 100 200)
100

> (define x 42)
42

> x
42

> (list 1 2 3)
(1 2 3)
```

## 実装済み機能

### Phase 1: ブート ✅
- [x] C/Assembly起動コード
- [x] UART初期化
- [x] Rustへのジャンプ

### Phase 2: Rustカーネル ✅
- [x] メモリアロケータ (Bump Allocator)
- [x] Panic Handler
- [x] UART/GPIO/Timerドライバ

### Phase 3: S式基盤 ✅ (v0.1)
- [x] S式パーサ
- [x] 評価器
- [x] 基本的な組み込み関数 (+, -, *, /, =, <, >, list, car, cdr)
- [x] 特殊形式 (define, if, quote, begin, set!)
- [x] REPL

### Phase 4: 高階関数 ✅ (v0.2)
- [x] **Lambda式** - 無名関数の定義
- [x] **クロージャ** - 環境をキャプチャする関数
- [x] **マクロ基盤** - defmacro構文（高度な機能はv0.3で実装予定）
- [x] 高階関数のサポート
- [x] レキシカルスコープ
- [x] 再帰関数のサポート
- [x] **高度なREPL機能**
  - [x] タブ補完（80+キーワード）
  - [x] コマンド履歴（↑↓キー、最大32履歴）
  - [x] 複数行入力（括弧の自動バランス）
  - [x] カーソル移動（←→キー）
  - [x] Ctrl+A/E/K/U/C/D
  - [x] 履歴永続化基盤

### Phase 5: 割込みとスケジューラ ✅ (v0.3)
- [x] **割込み処理**
  - [x] 例外ベクタテーブル (AArch64)
  - [x] IRQハンドラ
  - [x] コンテキスト保存・復元
- [x] **タイマー割込み**
  - [x] 10ms周期の定期割込み
  - [x] Tickカウンタ
- [x] **タスクスケジューラ基盤**
  - [x] タスク制御ブロック (TCB)
  - [x] コンテキストスイッチ
  - [x] ラウンドロビンスケジューラ
- [x] **Lisp API拡張**
  - [x] `(spawn fn)` - タスク生成（基盤）
  - [x] `(task-id)` - タスクID取得
  - [x] `(sleep ms)` - スリープ
  - [x] `(ticks)` - システムティック取得

### Phase 6: 今後の予定 🚧
- [ ] 完全なマルチタスク（自動タスク切り替え）
- [ ] タスク間通信（メッセージパッシング）
- [ ] 同期プリミティブ（ミューテックス、セマフォ）
- [ ] Lispクロージャのタスク化
- [ ] Quasiquote/Unquote（マクロの完全サポート）
- [ ] let/let* 構文
- [ ] 文字列操作関数
- [ ] タスク管理・協調マルチタスク
- [ ] フレームバッファドライバ
- [ ] USB対応
- [ ] ファイルシステム

## OS API (実装中/予定)

```lisp
; システム操作
(ticks)              ; システムティック数取得 (v0.3) ✅
(sleep 1000)         ; ミリ秒スリープ (v0.3) ✅

; タスク管理
(spawn fn)           ; タスク生成 (v0.3 基盤) ✅
(task-id)            ; 現在のタスクID (v0.3) ✅

; 予定
(os/print "Hello")   ; システム出力
(os/time)            ; 時刻取得

; GPIO操作
(os/gpio-mode 21 'output)
(os/gpio-write 21 1)
(os/gpio-read 21)

; メモリ管理
(memory/alloc 1024)
(memory/free ptr)
```

## 開発ロードマップ

- **v0.1** (現在) - 基本的なREPL、ドライバ、S式評価
- **v0.2** - Lambda、クロージャ、マクロ
- **v0.3** - タスクスケジューラ、割込み処理
- **v0.4** - グラフィック、USB
- **v0.5** - ファイルシステム、Self-hosting

## ライセンス

MIT License

## 貢献

Issue、Pull Requestを歓迎します！

## 参考資料

- [Raspberry Pi Bare Metal Programming](https://github.com/raspberrypi/documentation)
- [Writing an OS in Rust](https://os.phil-opp.com/)
- [Structure and Interpretation of Computer Programs](https://mitpress.mit.edu/sites/default/files/sicp/index.html)

---

**IndigoLispOS** - Where S-expressions meet bare metal 🚀
