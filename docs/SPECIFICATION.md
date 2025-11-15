# IndigoLispOS 仕様書

## 概要

- **ターゲット**: Raspberry Pi 5 / AArch64
- **出力**: kernel8.img
- **コンセプト**: S式がネイティブに動作するOS
- **実装言語**: C + AArch64 Assembly (初期化), Rust (コア部分)

## システムアーキテクチャ

### レイヤ構造

1. **ハードウェア層** - Raspberry Pi 5 (BCM2712)
2. **ブート層** - Assembly + C (boot.S, kernel.c)
3. **カーネル層** - Rust (no_std, no_main)
4. **ドライバ層** - UART, GPIO, Timer, など
5. **Lisp層** - Parser, Evaluator, REPL

### メモリマップ

```
0x00000000 - 0x0007FFFF : 予約領域
0x00080000 - 0x???????? : カーネルコード (.text)
           - 0x???????? : 読み取り専用データ (.rodata)
           - 0x???????? : データ (.data)
           - 0x???????? : BSS (.bss)
           - 0x???????? : スタック (1MB)
           - 0x???????? : ヒープ (16MB)
```

## ブートシーケンス

1. Raspberry Pi ファームウェアが kernel8.img を 0x80000 にロード
2. `_start` (boot.S) でアセンブリ実行開始
3. CPU0 以外のコアを停止
4. スタックポインタ設定
5. BSS セクションをゼロクリア
6. `kernel_main()` (kernel.c) 呼び出し
7. UART 初期化
8. MMU 初期化 (最小構成)
9. `rust_entry()` (lib.rs) 呼び出し
10. グローバルアロケータ初期化
11. ドライバ初期化
12. REPL 起動

## デバイスドライバ

### UART (PL011)

**ベースアドレス**: 0x107D001000

**レジスタ**:
- DR (0x00): データレジスタ
- FR (0x18): フラグレジスタ
- IBRD (0x24): 整数ボーレート除数
- FBRD (0x28): 小数ボーレート除数
- LCRH (0x2C): ライン制御
- CR (0x30): 制御レジスタ

**初期化**:
1. UART 無効化
2. ボーレート設定 (115200)
3. 8N1 設定 (8ビット, パリティなし, 1ストップビット)
4. UART 有効化

### GPIO

**ベースアドレス**: 0x107D508500

**機能**:
- ピンモード設定 (入力/出力/代替機能)
- ピンレベル設定 (HIGH/LOW)
- ピンレベル読み取り

**レジスタ**:
- GPFSEL0-5: 機能選択
- GPSET0-1: 出力セット
- GPCLR0-1: 出力クリア
- GPLEV0-1: ピンレベル

### RP1 Ethernet (Cadence GEM/MACB)

**物理ベースアドレス**: 0x1F00100000

**概要**:
- Raspberry Pi 5では、Gigabit Ethernetコントローラ(Cadence GEM)がRP1 I/Oコントローラチップ内に配置されている
- ARMコアから見た物理アドレスは 0x1F00100000 (バスマスターアドレス)
- Linuxのデバイスツリーでは `macb 1f00100000.ethernet` として表示される
- Pi 4の 0x3F000000 ベースアドレスから大幅に変更されている点に注意

**CPUメモリアドレスマッピング** (ベアメタルプログラミング用):
- CPUアドレス 0x1f_0000_0000 ↔ RP1プロセッサアドレス 0x4000_0000
- このマッピングはPCIeアウトバウンドウィンドウによって設定される
- 例: UART0 = 0x1f_0003_0000, GPIO = 0x1f_000d_0000, Ethernet = 0x1f_001c_0000

**アクセス方法**:
- RP1チップはPCIe経由でアクセス
- ベースアドレス + 0x001c0000 がEthernet/MACBレジスタ領域
- BARマッピングを通じてアクセスするのが標準的な方法

**主要レジスタ** (Cadence GEM標準):
- NWCTRL (0x000): ネットワーク制御
- NWCFG (0x004): ネットワーク設定
- NWSR (0x008): ネットワークステータス
- DMACFG (0x010): DMA設定
- RXQBASE (0x018): 受信キューベースアドレス
- TXQBASE (0x01C): 送信キューベースアドレス
- SPADDR1LO/HI (0x088/0x08C): MACアドレス設定

**注意事項**:
- Raspberry Pi 5では全てのアドレスがバスマスターアドレスとして扱われる
- ベアメタルプログラミングでは、従来モデルとのアドレス体系の違いに注意が必要

### Timer

**ベースアドレス**: 0x107C003000

**機能**:
- システム時刻取得 (マイクロ秒)
- ビジーウェイト遅延

## Lisp 評価器

### データ型

```rust
enum Expr {
    Number(i64),        // 整数
    Symbol(String),     // シンボル
    String(String),     // 文字列
    Bool(bool),         // 真偽値
    List(Vec<Expr>),    // リスト
    Nil,                // nil
    Function(...),      // 関数
}
```

### 特殊形式

- `quote` - 式を評価せずに返す
- `define` - 変数定義
- `if` - 条件分岐
- `lambda` - 関数定義 (部分実装)
- `begin` - 順次実行
- `set!` - 変数更新

### 組み込み関数

**算術演算**:
- `+` - 加算
- `-` - 減算
- `*` - 乗算
- `/` - 除算

**比較演算**:
- `=` - 等価比較
- `<` - 小なり
- `>` - 大なり

**リスト操作**:
- `list` - リスト生成
- `car` - リストの先頭要素
- `cdr` - リストの残り

### パーサ

**字句解析**:
- トークン化 (括弧、数値、シンボル、文字列)
- 空白文字の処理
- 文字列リテラルのサポート

**構文解析**:
- S式 → AST (抽象構文木)
- エラーハンドリング

### 評価戦略

1. 自己評価型 (数値、文字列、真偽値) → そのまま返す
2. シンボル → 環境から値を探索
3. リスト → 特殊形式チェック → 関数適用

### 環境 (Environment)

- 変数バインディングの管理
- スコープチェーン (親環境への参照)
- `define` で新規バインディング作成
- `set!` で既存バインディング更新

## ビルドシステム

### Makefile ターゲット

- `all` (デフォルト) - kernel8.img をビルド
- `rust` - Rust コンポーネントのみビルド
- `deploy` - SD カードにデプロイ
- `clean` - ビルド成果物を削除

### 依存関係

```
kernel8.img
 ├── kernel8.elf (リンク)
 │    ├── boot.o (Assembly)
 │    ├── kernel.o (C)
 │    ├── uart.o (C)
 │    ├── mmu.o (C)
 │    └── libindigo_lisp_os.a (Rust)
 │         ├── lib.rs
 │         ├── allocator.rs
 │         ├── panic.rs
 │         ├── drivers/*
 │         └── lisp/*
 └── (objcopy)
```

## 制約事項

### 現在の制限

- シングルコア動作のみ
- MMU は最小構成
- アロケータは Bump Allocator (解放なし)
- Lambda/クロージャは未実装
- タスクスケジューリング未実装
- 割込み処理は最小限

### Raspberry Pi 5 固有の課題

- ペリフェラルのベースアドレスが Pi 4 から変更
- 正確なアドレスマップは要検証

## 将来の拡張

### Phase 4: タスク管理

- 協調的マルチタスク
- タイマー割込みによるプリエンプション
- タスク切り替え (コンテキストスイッチ)

### Phase 5: グラフィック

- フレームバッファドライバ
- 基本的な描画 API
- GUI REPL

### Phase 6: ファイルシステム

- SD カードドライバ
- FAT32 サポート
- Lisp ファイルのロード・実行

### Phase 7: Self-hosting

- OS 拡張を Lisp で記述
- ブートストラップ Lisp
- マクロシステム

## 参考文献

- [ARM Architecture Reference Manual](https://developer.arm.com/documentation/)
- [BCM2711 ARM Peripherals](https://www.raspberrypi.org/documentation/hardware/raspberrypi/)
- [Rust Embedded Book](https://rust-embedded.github.io/book/)
- [SICP - Structure and Interpretation of Computer Programs](https://mitpress.mit.edu/sites/default/files/sicp/index.html)

## バージョン履歴

- **v0.1** (Draft) - 初期実装
  - ブート処理
  - 基本ドライバ (UART, GPIO, Timer)
  - S式評価器
  - REPL

---

*最終更新: 2025年11月15日*
