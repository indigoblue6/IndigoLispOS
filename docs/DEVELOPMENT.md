# 開発ガイド - IndigoLispOS

## セットアップ

### 1. 開発環境の準備

```bash
# AArch64クロスコンパイラのインストール
sudo apt update
sudo apt install -y gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu

# Rustのインストール (未インストールの場合)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# AArch64ターゲットの追加
rustup target add aarch64-unknown-none
```

### 2. プロジェクトのビルド

```bash
cd IndigoLispOS
make
```

ビルドに成功すると `kernel8.img` が生成されます。

## コード構造の理解

### ブートプロセス

1. **boot/boot.S** - AArch64アセンブリで起動
   - CPU IDチェック (CPU0のみ起動)
   - スタック設定
   - BSS セクションのゼロクリア
   - C言語の `kernel_main()` 呼び出し

2. **src-c/kernel.c** - C言語での初期化
   - UART初期化 (デバッグ出力用)
   - MMU初期化 (最小構成)
   - Rustエントリポイント `rust_entry()` 呼び出し

3. **src-rust/src/lib.rs** - Rustカーネル本体
   - グローバルアロケータ初期化
   - ドライバ初期化
   - REPL起動

### メモリレイアウト

`arch/aarch64/linker.ld` で定義:

```
0x80000      - カーネルコード開始 (.text)
             - 読み取り専用データ (.rodata)
             - 初期化済みデータ (.data)
             - 未初期化データ (.bss)
__stack_top  - スタック終端 (1MB)
__heap_start - ヒープ開始
__heap_end   - ヒープ終端 (16MB)
```

### Rustモジュール構成

```
src-rust/src/
├── lib.rs         - メインエントリ、カーネルループ
├── allocator.rs   - Bump Allocator実装
└── panic.rs       - パニックハンドラ

drivers/
├── uart.rs        - UART PL011ドライバ
├── gpio.rs        - GPIOドライバ
├── timer.rs       - システムタイマー
└── mod.rs         - モジュール集約

lisp/
├── expr.rs        - S式データ型定義
├── parser.rs      - 字句解析・構文解析
├── env.rs         - 環境・変数バインディング
├── eval.rs        - 評価器・組み込み関数
└── mod.rs         - モジュール集約
```

## 新機能の追加

### 新しい組み込み関数の追加

`lisp/eval.rs` の `init_builtins()` に追加:

```rust
env.define("my-func".to_string(), Expr::Function(Rc::new(|args| {
    // 引数チェック
    if args.len() != 2 {
        return Err("my-func requires 2 arguments".to_string());
    }
    
    // 処理を実装
    // ...
    
    Ok(result)
})));
```

### 新しいドライバの追加

1. `drivers/` に新しいファイルを作成 (例: `spi.rs`)
2. ドライバ構造体とメソッドを実装
3. `drivers/mod.rs` でモジュールをエクスポート

```rust
// drivers/spi.rs
use core::ptr;

const SPI_BASE: usize = 0x...;

pub struct Spi;

impl Spi {
    pub fn new() -> Self {
        Spi
    }
    
    pub fn init(&self) {
        // 初期化処理
    }
    
    // その他のメソッド
}

pub static SPI: Spi = Spi;
```

### OS APIの追加

Lispから呼び出せるOS機能を追加する場合:

1. `lisp/eval.rs` の `init_builtins()` に関数を追加
2. ドライバを呼び出す

```rust
env.define("os/led-on".to_string(), Expr::Function(Rc::new(|args| {
    use crate::drivers::gpio::{GPIO, GpioFunction};
    
    if args.len() != 1 {
        return Err("os/led-on requires pin number".to_string());
    }
    
    match &args[0] {
        Expr::Number(pin) => {
            GPIO.set_function(*pin as u32, GpioFunction::Output);
            GPIO.set(*pin as u32);
            Ok(Expr::Nil)
        }
        _ => Err("Pin must be a number".to_string()),
    }
})));
```

## デバッグ

### UARTデバッグ出力

Rustコード内で `println!` マクロを使用:

```rust
use crate::println;

println!("Debug: value = {}", value);
```

### パニック時の情報

`src-rust/src/panic.rs` でパニック情報をUARTに出力。
ファイル名、行番号が表示されます。

### QEMUデバッグ

```bash
# GDBサーバーモードで起動
qemu-system-aarch64 \
    -M raspi3b \
    -kernel kernel8.img \
    -serial stdio \
    -s -S

# 別ターミナルでGDB接続
aarch64-linux-gnu-gdb build/kernel8.elf
(gdb) target remote localhost:1234
(gdb) break rust_entry
(gdb) continue
```

## テスト

### 単体テスト

Rustコード内でテストを書く (注: `no_std` 環境での制約あり):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser() {
        // テストコード
    }
}
```

ホスト環境でテスト:

```bash
cd lisp
cargo test --target x86_64-unknown-linux-gnu
```

### 統合テスト

実機またはQEMUで REPL から手動テスト:

```lisp
> (+ 1 2 3)
6
> (define test-var 42)
42
> test-var
42
```

## パフォーマンス最適化

### リリースビルド

Makefile はデフォルトでリリースビルド (`--release`) を使用。
`Cargo.toml` で最適化レベルを調整可能:

```toml
[profile.release]
opt-level = "z"  # サイズ最適化
lto = true       # Link Time Optimization
```

### メモリ使用量の削減

- 現在のアロケータは Bump Allocator (解放なし)
- より高度なアロケータ (Slab, Buddy) への置き換えを検討

## トラブルシューティング

### ビルドエラー

**`aarch64-none-elf-gcc not found` または `aarch64-linux-gnu-gcc not found`**
```bash
sudo apt install -y gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu
```

Makefileは`aarch64-linux-gnu-gcc`を使用するように設定されています。

**Rustターゲットがない**
```bash
rustup target add aarch64-unknown-none
```

### 実行時エラー

**UARTに何も表示されない**
- ボーレート設定を確認 (115200)
- UART0のベースアドレスを確認 (Raspberry Pi 5 仕様)
- シリアルケーブルの接続を確認

**カーネルパニック**
- パニックメッセージをUARTで確認
- スタックオーバーフローの可能性 → リンカスクリプトでスタックサイズを増やす

## コントリビューション

プルリクエストを送る前に:

1. コードフォーマット: `cargo fmt`
2. リントチェック: `cargo clippy`
3. ビルド確認: `make clean && make`
4. 動作確認: QEMUまたは実機でテスト

## 次のステップ

- [ ] Lambda/クロージャの完全実装
- [ ] タスクスケジューラ
- [ ] 割込み処理の完全実装
- [ ] フレームバッファドライバ
- [ ] SD カードドライバ
- [ ] ファイルシステム

詳細は [README.md](README.md) のロードマップを参照してください。
