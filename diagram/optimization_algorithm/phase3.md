# フェーズ3: 精密探索アルゴリズム

## 精密探索の概要

フェーズ3は、フェーズ2の黄金分割探索によって狭められた範囲内で、最大値をより精密に探索するためのフェーズです。特に、上限閾値（ゼロに落ちる境界）近傍を重点的に調査します。

```mermaid
graph TD
    A(["フェーズ2から"]) --> B["狭い区間内での精密探索"]
    B --> C["区間内の各整数点を評価"]
    C --> D["最高評価値とその位置を記録"]
    D --> E{"上限閾値<br/>(ゼロ点)発見?"}
    E -->|"はい"| F["境界直前の点を特定"]
    E -->|"いいえ"| G["探索範囲を少し拡張"]
    G --> H{"拡張範囲で<br/>ゼロ点発見?"}
    H -->|"はい"| F
    H -->|"いいえ"| I["最良値で確定"]
    F --> J["最良値の最終確認"]
    I --> J
    J --> K(["終了"])
```

## シンプルな精密探索アルゴリズム

フェーズ3では、以下のシンプルな手順で精密探索を行います：

1. フェーズ2から受け取った狭い区間内（通常は幅が小さい）の整数点を順次評価します
2. 評価しながら最高評価値とその位置を常に記録します
3. 評価値がゼロになる点（上限閾値）を発見した場合、その直前の点を「境界点」として特に注目します
4. 区間内で上限閾値が見つからない場合は、少し探索範囲を拡張して上限閾値を探します

## 詳細アルゴリズム

```mermaid
graph TD
    A(["開始：フェーズ2から狭い区間を受け取る"]) --> B["区間内の各整数点を評価"]
    B --> C["最高評価値とその位置を記録"]
    C --> D["区間内で右側へ順次1ステップずつ探索"]
    D --> E{"評価値がゼロか?"}
    
    E -->|"いいえ"| F["評価結果を記録"]
    F --> G{"評価値が以前より高いか?"}
    G -->|"はい"| H["最高評価値と位置を更新"]
    G -->|"いいえ"| I["変更なし"]
    
    H --> J{"区間の右端まで探索したか?"}
    I --> J
    
    J -->|"いいえ"| D
    J -->|"はい"| K["最良位置から右側へさらに探索"]
    
    E -->|"はい"| L["上限閾値発見:\n直前の点を境界点として記録"]
    L --> M["境界点から数ステップ戻って再評価"]
    
    K --> N{"ゼロになる点を発見したか?"}
    N -->|"はい"| L
    N -->|"いいえ"| O["追加探索で最高値を更新"]
    
    M --> P(["終了：最高評価値とその位置を返却"])
    O --> P
```

## 終了条件

フェーズ3の探索は、以下のいずれかの条件が満たされた時点で終了します：

### 1. 上限閾値の発見
評価値がゼロになる点を見つけた場合、その直前の点が上限閾値の直前と判断します。この点を境界点として記録し、境界点の周辺数点を再評価した後に探索を終了します。

### 2. 区間全体の探索完了
与えられた区間内のすべての整数点を評価し終えた場合、探索範囲をわずかに拡張して上限閾値を探します。拡張範囲でも上限閾値が見つからない場合は、最高評価値を持つ点で探索を終了します。

### 3. 再評価と最終確認
上限閾値が見つかった場合、境界点から数ステップだけ戻った点も評価して、境界近くでの最適値を正確に把握します。これにより、最終的な最良値と、1ステップ先がゼロになる境界点を正確に特定します。

### 4. 結果返却
探索プロセス全体を通じて見つかった最高評価値とその位置を最終結果として返却します。
