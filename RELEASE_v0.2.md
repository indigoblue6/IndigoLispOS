# IndigoLispOS v0.2 Release Notes

**リリース日**: 2025年11月15日

## 🎉 新機能

### Lambda式とクロージャ
- **Lambda式の実装**: 無名関数を定義できるようになりました
- **レキシカルスコープ**: 環境をキャプチャするクロージャのサポート
- **高階関数**: 関数を引数として渡したり、関数を返したりできます
- **再帰関数**: factorial などの再帰的な関数定義が可能

### マクロ基盤
- **defmacro構文**: マクロ定義の基本構造を実装
- **マクロ展開機能**: 評価前にマクロが自動的に展開されます
- **注記**: 高度なマクロ機能（quasiquote/unquote）はv0.3で実装予定

### 高度なREPL機能
- **タブ補完**: 80+のキーワード（define, lambda, if, +, -, list, car, cdrなど）
- **コマンド履歴**: ↑↓キーで過去のコマンドを参照（最大32履歴）
- **複数行入力**: 括弧が閉じるまで自動的に入力を継続（`...` プロンプト）
- **カーソル移動**: ←→キーでカーソル移動、Backspace/Deleteで編集
- **Emacsキーバインド**: Ctrl+A(行頭), E(行末), K(行末まで削除), U(全削除), C(キャンセル), D(削除)
- **履歴永続化基盤**: メモリ上に履歴を保持

## 📝 使用例

### Lambda式
```lisp
> (define add (lambda (x y) (+ x y)))
<lambda>

> (add 3 4)
7

> (define square (lambda (x) (* x x)))
<lambda>

> (square 5)
25
```

### クロージャ
```lisp
> (define make-adder (lambda (n) (lambda (x) (+ x n))))
<lambda>

> (define add5 (make-adder 5))
<lambda>

> (add5 10)
15

> (define add100 (make-adder 100))
<lambda>

> (add100 23)
123
```

### 再帰関数
```lisp
> (define factorial (lambda (n) (if (< n 2) 1 (* n (factorial (- n 1))))))
<lambda>

> (factorial 5)
120

> (factorial 10)
3628800
```

### マクロ（基本）
```lisp
; マクロ定義の構文は利用可能
> (defmacro name (params) body)
<macro>

; 注: 高度なマクロ機能（quasiquote/unquoteを使うマクロ）は
; v0.3で実装予定です
```

## 🔧 技術的な改善

### アーキテクチャ
- **環境スナップショット**: クロージャが環境をキャプチャする仕組みを実装
- **EnvSnapshot構造体**: 最大16個の変数バインディングを保存
- **レキシカルスコープ**: 関数定義時の環境を保持
- **高度なREPLエディタ**: ANSI エスケープシーケンス対応のライン編集機能

### 式の型拡張
- `Expr::Lambda`: パラメータ、ボディ、キャプチャされた環境を保持
- `Expr::Macro`: パラメータとボディを保持（環境キャプチャなし）

### 評価器の拡張
- `eval_lambda`: lambda式を評価してクロージャを作成
- `eval_defmacro`: マクロ定義を処理
- `expand_macro`: マクロを展開してコード変換
- `apply_lambda`: クロージャを適用（グローバル環境も参照して再帰をサポート）

### REPLの拡張
- `ReplEditor`: 入力エディタクラス
- `read_line`: 高機能な行入力
- `handle_tab_completion`: タブ補完ロジック
- `history_up/down`: 履歴ナビゲーション
- `is_balanced`: 括弧バランスチェック

## 📊 制約事項

### メモリ制約（no_std環境）
- 最大パラメータ数: 4個
- 最大リストアイテム: 8個
- 環境スナップショット: 16個のバインディング
- シンボル名長: 64文字

### 現在未サポート
- 可変長引数
- let/let* 構文（defineとlambdaで代用可能）
- **quasiquote/unquote**（マクロで必要 - v0.3で実装予定）
- 末尾再帰最適化

## 🚀 次のステップ（v0.3以降）

### Phase 5: Lisp機能拡張
- [ ] **Quasiquote/Unquote** - マクロの完全サポート
- [ ] **let/let*** - ローカルバインディング
- [ ] より多くの組み込み関数
- [ ] 文字列操作関数
- [ ] エラーハンドリングの改善
- [ ] デバッグ機能

### Phase 6: タスク管理
- [ ] 協調的マルチタスク
- [ ] タイマー割り込み
- [ ] プリエンプティブスケジューリング

## 📦 ビルド

```bash
make clean
make
```

生成物: `kernel8.img` (Raspberry Pi 5用カーネルイメージ)

## 🎯 動作確認

examples/example.lisp に多数のテストケースが含まれています。
REPLで直接試すことができます。

---

**完全実装機能一覧**:
- ✅ Lambda式
- ✅ クロージャ
- ✅ マクロ基盤 (defmacro構文)
- ✅ 高階関数
- ✅ レキシカルスコープ
- ✅ 再帰関数
- ✅ 環境キャプチャ
- ✅ **タブ補完（80+キーワード）**
- ✅ **コマンド履歴（↑↓キー）**
- ✅ **複数行入力（括弧バランス）**
- ✅ **カーソル移動・編集**
- ✅ **Emacsキーバインド**
- ⏳ マクロ展開（quasiquote/unquoteはv0.3で実装予定）

**IndigoLispOS v0.2 - S式がネイティブに動作するOS**
