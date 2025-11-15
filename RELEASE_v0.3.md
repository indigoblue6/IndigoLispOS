# IndigoLispOS v0.3 Release Notes

**リリース日**: 2025年11月15日

## 🎉 新機能

### 割込み処理システム
- **例外ベクタテーブル**: AArch64の完全な例外ベクタテーブル実装
- **IRQハンドラ**: 割込み時のコンテキスト保存・復元
- **タイマー割込み**: 10ms周期の定期タイマー割込み
- **Tick Count**: システムティック数の管理とアクセス

### タスクスケジューラ基盤
- **タスク制御ブロック(TCB)**: タスク状態とコンテキストの管理
- **コンテキストスイッチ**: タスク間のレジスタ保存・復元機構
- **ラウンドロビンスケジューラ**: シンプルな協調的タスク切り替え
- **アイドルタスク**: システムアイドル時の省電力タスク

### Lisp APIの拡張
- **`(spawn function)`**: 新しいタスクを生成（基盤実装）
- **`(task-id)`**: 現在のタスクIDを取得
- **`(sleep ms)`**: 指定ミリ秒スリープ
- **`(ticks)`**: システムティック数を取得

## 📝 技術詳細

### 割込み処理アーキテクチャ

```
割込み発生
    ↓
例外ベクタテーブル (VBAR_EL1)
    ↓
コンテキスト保存 (272バイト)
    ↓
IRQハンドラ (Rust)
    ↓
タイマー割込み処理
    ↓
コンテキスト復元
    ↓
ERET (例外からの復帰)
```

#### 保存されるレジスタ
- 汎用レジスタ: x0-x30 (31個)
- スタックポインタ: SP
- 例外復帰アドレス: ELR_EL1
- プログラムステータス: SPSR_EL1

### タイマー割込み

```lisp
> (ticks)
0

> (sleep 100)
nil

> (ticks)
10  ; 100ms = 10 ticks (10ms/tick)
```

**仕様**:
- 周期: 10ms (100Hz)
- タイマー: ARM Generic Timer (CNTVTVAL_EL0)
- 周波数: 54MHz (Raspberry Pi 5)

### タスクスケジューラ

#### タスク構造体
```rust
pub struct Task {
    pub id: usize,              // タスクID
    pub state: TaskState,       // 状態 (Ready/Running/Blocked/Terminated)
    pub context: TaskContext,   // CPUコンテキスト
    pub stack: Box<[u8]>,       // スタック (8KB)
    pub name: &'static str,     // タスク名
}
```

#### コンテキスト
```rust
pub struct TaskContext {
    pub x19-x28: u64,  // Callee-saved registers
    pub x29: u64,      // Frame Pointer
    pub x30: u64,      // Link Register (戻りアドレス)
    pub sp: u64,       // Stack Pointer
}
```

#### スケジューリング
- **方式**: ラウンドロビン
- **タイミング**: 明示的yield時（将来はタイマー割込み時に自動）
- **アイドルタスク**: タスク0として常に存在

## 🔧 使用例

### 基本的な割込み機能

```lisp
> (ticks)
0

> (define start-time (ticks))
<number>

> (sleep 500)
nil

> (- (ticks) start-time)
50  ; 500ms経過 = 50 ticks
```

### タスク関連API

```lisp
> (task-id)
0  ; メインタスクのID

> (spawn (lambda () (+ 1 2)))
1  ; 新しいタスクID（基盤実装のみ）
```

## 🏗️ アーキテクチャ更新

```
┌─────────────────────────────────────┐
│    S式 REPL / Evaluator (Lisp)     │
│  新API: spawn, task-id, sleep      │
├─────────────────────────────────────┤
│        Task Scheduler (Rust)        │ ← 新規
│   Round-Robin, Context Switch       │
├─────────────────────────────────────┤
│      Interrupt Handler (Rust)       │ ← 新規
│    IRQ Processing, Timer ISR        │
├─────────────────────────────────────┤
│   Exception Vectors (Assembly)      │ ← 新規
│    VBAR_EL1, Context Save/Restore   │
├─────────────────────────────────────┤
│    Drivers (UART/GPIO/Timer)        │
│  Timer: 割込み対応                  │ ← 更新
├─────────────────────────────────────┤
│    Kernel Runtime & Allocator       │
├─────────────────────────────────────┤
│    Boot & Hardware Init             │
└─────────────────────────────────────┘
```

## 📂 新規ファイル

```
arch/aarch64/
  └── interrupts.S          # 割込みベクタテーブルとハンドラ

src-rust/src/
  ├── interrupt.rs          # 割込み管理モジュール
  └── scheduler.rs          # タスクスケジューラ
```

## 🔄 変更されたファイル

- `src-rust/src/lib.rs`: 割込み・スケジューラモジュール統合
- `drivers/timer.rs`: 割込み機能追加、ティックカウンタ
- `lisp/eval.rs`: 新Lisp関数追加 (spawn, task-id, sleep, ticks)
- `Makefile`: interrupts.Sのビルド追加

## ⚙️ ビルドと実行

### ビルド
```bash
make clean
make
```

### デプロイ
```bash
make deploy SD_MOUNT=/path/to/sd
```

### 実行
シリアルコンソール (115200 baud) で接続後:
```
Welcome to IndigoLispOS REPL v0.3
Features: Interrupts, Task Scheduler, Lambda, Macros
New: (spawn fn), (task-id), (sleep ms), (ticks)
Type S-expressions to evaluate

> (ticks)
0
> (sleep 1000)
nil
> (ticks)
100
```

## 🚀 今後の予定 (v0.4)

- **完全なマルチタスク**: タイマー割込みによる自動タスク切り替え
- **タスク間通信**: メッセージパッシング、共有メモリ
- **同期プリミティブ**: ミューテックス、セマフォ
- **Lispタスク**: Lispクロージャを直接タスクとして実行
- **優先度スケジューリング**: プライオリティベースのスケジューリング

## 📊 統計

- **総コード行数**: ~1,500行
- **新規追加**: ~600行
- **Lispビルトイン関数**: 20+ (spawn, task-id, sleep, ticks 追加)
- **割込みベクタ**: 16エントリ
- **タイマー周期**: 10ms (100Hz)
- **タスクスタック**: 8KB/タスク

## 🐛 既知の問題

1. **スケジューラ統合**: タイマー割込み時の自動タスク切り替えは未実装
2. **Lispタスク**: `spawn`はプレースホルダ実装のみ
3. **タスク終了**: タスク終了処理が未完成
4. **同期**: 排他制御機構が未実装

## 📚 参考資料

- ARM Architecture Reference Manual (ARMv8-A)
- BCM2712 ARM Peripherals (Raspberry Pi 5)
- Rust Embedded Book

---

**v0.2からv0.3への移行**: 既存のLispコードは完全に互換性があります。新しい割込み・タスク機能を活用できます。
