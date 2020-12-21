# HAMELN-PUBLISH

小説投稿サイト [HAMELN](https://syosetu.org/) の作品から ePub 形式の電子書籍データを生成するツールです。

## 使い方

コマンドラインから以下のように入力するとカレントディレクトリに ePub 形式のデータが出力されます。

```
hameln-publish [作品ID] ...
```

出力されるファイル名は

```
[作者名] 表題.epub
```

のような形式をとります。
