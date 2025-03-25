# Local LLM の検討

ここでは Gemma 3 と Phi 4 を対象として検討する。

## Gemma 3 のモデルの比較

[公式サイト](https://ai.google.dev/gemma/docs/core?hl=ja#sizes)より

| パラメータ | 32 ビット | BF16（16 ビット） | SFP8（8 ビット） | Q4_0（4 ビット） | INT4（4 ビット） |
| --- | --- | --- | --- | --- | --- |
| Gemma 3 1B（テキストのみ） | 4 GB | 1.5 GB | 1.1 GB | 892 MB | 861 MB |
| Gemma 3 4B | 16 GB | 6.4 GB | 4.4 GB | 3.4 GB | 3.2 GB |
| Gemma 3 12B | 48 GB | 20 GB | 12.2 GB | 8.7 GB | 8.2 GB |
| Gemma 3 27B | 108 GB | 46.4 GB | 29.1 GB | 21 GB | 19.9 GB |

動かすマシンは 32GB を想定する。
空きメモリが 16GB 程度と考えると、以下が対象となる。
Q4_0 と INT4 はメモリがそんなに変わらないが Q4_0 の方が精度がいいらしいので、INT4 は選択肢にいれない。

| モデル | メモリ |
| --- | --- |
| 12B-SFP8 | 12.2GB |
| 12B-Q4_0 | 8.7GB |
| ~~12B-INT4~~ | ~~8.2GB~~ |

ollama pull でインストールすると Q4_K_M になる。[他の量子化も選べるようだが](https://ollama.com/library/gemma3/tags)これを使うことにする。

## Phi 4 のモデルの比較

これも ollama pull でインストールすると Q4_K_M になる。
これを使うことにする。
https://ollama.com/library/phi4/tags
