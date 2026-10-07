import { MarkdownDoc, MarkdownBlock, MarkdownRun, MarkdownCell, MarkdownRow, MarkdownAttachment, resolveMarkdownImage } from '../model/Markdown';
import { VerifiedImageView } from '../model/VerifiedImages';
import { alpha } from '../model/Appearance';
import { uiText } from '../model/UiStrings';

@Component
export struct MarkdownPreview {
  @Prop @Watch('documentChanged') document: MarkdownDoc = new MarkdownDoc();
  @Prop ink: string = '#302D43';
  @Prop muted: string = '#81798F';
  @Prop accent: string = '#7662BA';
  @Prop line: string = '#227662BA';
  @Prop family: string = 'HarmonyOS Sans';
  @Prop locale: string = 'zh';
  openLink: (href: string) => void = (): void => {};
  @Prop attachments: MarkdownAttachment[] = [];
  @Prop images: VerifiedImageView[] = [];
  openImage: (id: string) => void = (): void => {};
  retryImage: (id: string) => void = (): void => {};
  imageError: (id: string, path: string) => void = (): void => {};
  @State requestedImages: string[] = [];
  @State failedImages: string[] = [];
  private imageEpoch: number = 0;

  private documentChanged(): void { this.imageEpoch++; this.requestedImages = []; this.failedImages = []; }
  private imageFailure(run: MarkdownRun, epoch: number): () => void {
    return (): void => {
      if (epoch === this.imageEpoch && this.requestedImages.includes(run.href)) { this.failedImages = this.failedImages.concat([run.href]); }
    };
  }
  private canLoadImage(run: MarkdownRun): boolean { return resolveMarkdownImage(run.href, this.attachments).kind === 'remote'; }
  private attachmentId(run: MarkdownRun): string { return resolveMarkdownImage(run.href, this.attachments).attachment_id; }
  private imageView(run: MarkdownRun): VerifiedImageView | undefined {
    return this.images.find((value: VerifiedImageView): boolean => value.id === this.attachmentId(run));
  }
  private localImageFailure(id: string, path: string): () => void { return (): void => this.imageError(id, path); }
  private hasText(runs: MarkdownRun[]): boolean { return runs.some((run: MarkdownRun): boolean => !run.image && run.text.length > 0); }
  private blockFontSize(block: MarkdownBlock): number { return block.kind === 'heading' ? [25, 21, 18, 16, 14, 13][Math.max(0, Math.min(5, block.level - 1))] : 13; }
  private cellAlign(value: string): TextAlign { return value === 'right' ? TextAlign.End : value === 'center' ? TextAlign.Center : TextAlign.Start; }

  @Builder
  Runs(runs: MarkdownRun[], size: number = 13, strong: boolean = false, alignment: TextAlign = TextAlign.Start) {
    Text() {
      ForEach(runs, (run: MarkdownRun) => {
        Span(run.image ? '' : run.text)
          .fontFamily(run.code ? 'monospace' : this.family).fontSize(run.code ? 12 : size)
          .fontWeight(run.bold || strong ? FontWeight.Bold : FontWeight.Normal)
          .fontStyle(run.italic ? FontStyle.Italic : FontStyle.Normal)
          .fontColor(run.href.length > 0 || run.code ? this.accent : this.ink)
          .textBackgroundStyle({ color: run.code ? alpha(this.ink, .045) : Color.Transparent, radius: 3 })
          .decoration({ type: run.strike ? TextDecorationType.LineThrough : run.href && !run.image ? TextDecorationType.Underline : TextDecorationType.None })
          .onClick(() => { if (run.href && !run.image && !run.href.startsWith('attachment:')) { this.openLink(run.href); } })
      })
    }.fontSize(size).lineHeight(size * 1.8).width('100%').textAlign(alignment).copyOption(CopyOptions.LocalDevice)
  }
  @Builder
  Images(runs: MarkdownRun[]) {
    ForEach(runs.filter((run: MarkdownRun) => run.image), (run: MarkdownRun) => {
      if (this.attachmentId(run)) {
        if (this.imageView(run)?.phase === 'ready') {
          ForEach([this.imageView(run)!], (view: VerifiedImageView) => {
            Image(view.uri).objectFit(ImageFit.Contain).constraintSize({ maxHeight: 300 }).width('100%')
              .accessibilityText(run.text || this.t('图片')).id('markdown-attachment-image:' + view.id)
              .onError(this.localImageFailure(view.id, view.path)).onClick(() => this.openImage(view.id))
          }, (view: VerifiedImageView): string => view.path)
        } else {
          Button(this.imageView(run)?.phase === 'failed' ? this.t('图片读取失败') + ' · ' + (run.text || this.t('图片')) :
            this.imageView(run) ? this.t('正在读取图片…') : this.t('图片未导入') + ' · ' + (run.text || this.t('图片')), { type: ButtonType.Normal })
            .fontSize(12).fontColor(this.muted).backgroundColor(Color.Transparent).padding(10).borderRadius(8)
            .id('markdown-attachment-status:' + this.attachmentId(run))
            .enabled(this.imageView(run)?.phase === 'failed').onClick(() => this.retryImage(this.attachmentId(run)))
          if (this.imageView(run)?.error) { Text(this.imageView(run)!.error).fontSize(11).fontColor(this.muted) }
        }
      } else if (this.canLoadImage(run) && this.requestedImages.includes(run.href) && !this.failedImages.includes(run.href)) {
        Image(resolveMarkdownImage(run.href, this.attachments).href).objectFit(ImageFit.Contain).constraintSize({ maxHeight: 220 }).width('100%')
          .accessibilityText(run.text || this.t('图片'))
          .onError(this.imageFailure(run, this.imageEpoch))
      } else {
        Button(this.failedImages.includes(run.href) ? this.t('图片读取失败') : this.canLoadImage(run) ? this.t('读取图片') + ' · ' + (run.text || this.t('图片')) : this.t('图片未导入') + ' · ' + (run.text || this.t('图片')), { type: ButtonType.Normal })
          .fontSize(12).fontColor(this.muted).backgroundColor(Color.Transparent).border({ width: 1, color: this.line })
          .padding(10).borderRadius(8).enabled(this.canLoadImage(run) && !this.failedImages.includes(run.href))
          .onClick(() => { if (this.canLoadImage(run)) { this.requestedImages = this.requestedImages.concat([run.href]); } })
      }
    })
  }
  @Builder
  Table(block: MarkdownBlock) {
    Scroll() {
      Column() {
        ForEach(block.rows, (row: MarkdownRow, index: number) => {
          Row() {
            ForEach(row.cells, (cell: MarkdownCell, column: number) => {
              Column({ space: 8 }) { if (this.hasText(cell.runs)) { this.Runs(cell.runs, 13, row.header, this.cellAlign(block.alignments[column])); } this.Images(cell.runs) }
                .width(150).padding(9).alignItems(HorizontalAlign.Start)
                .border({ width: .5, color: this.line }).backgroundColor(row.header ? alpha(this.accent, .065) : Color.Transparent)
            })
          }.alignItems(VerticalAlign.Top).id('markdown-table-row:' + index.toString())
        })
      }.alignItems(HorizontalAlign.Start)
    }.scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Auto).width('100%').id('markdown-table')
  }
  private t(text: string): string { return uiText(text, this.locale); }
  build() {
    Column({ space: 12 }) {
      ForEach(this.document.blocks, (block: MarkdownBlock, index: number) => {
        Row({ space: 8 }) {
          if (block.marker.length > 0) { Text(block.marker).fontSize(13).fontColor(this.ink).width(30).padding({ top: 2 }) }
          else if (block.indent > 0) { Blank().width(30) }
          Column({ space: 8 }) {
            if (block.kind === 'rule') { Divider().color(this.line).margin({ top: 6, bottom: 6 }) }
            else if (block.kind === 'table') { this.Table(block) }
            else if (block.kind === 'code') {
              if (block.language) { Text(block.language).fontSize(10).fontColor(this.muted) }
              Text(block.runs.map((run: MarkdownRun) => run.text).join('')).fontFamily('monospace').fontSize(12)
                .fontColor(this.accent).lineHeight(20).width('100%').copyOption(CopyOptions.LocalDevice)
            } else { if (this.hasText(block.runs)) { this.Runs(block.runs, this.blockFontSize(block), block.kind === 'heading'); } this.Images(block.runs) }
          }.layoutWeight(1).alignItems(HorizontalAlign.Start)
            .padding(block.kind === 'code' || block.quote > 0 ? 12 : 0)
            .backgroundColor(block.kind === 'code' ? alpha(this.ink, .045) : block.quote > 0 ? alpha(this.accent, .065) : Color.Transparent)
            .borderRadius(block.kind === 'code' ? 12 : 0)
            .border({ width: { left: block.quote > 0 ? 3 : 0 }, color: this.accent })
        }.width('100%').alignItems(VerticalAlign.Top).padding({ left: block.indent * 16 + Math.max(0, block.quote - 1) * 12 })
          .id('markdown-block:' + index.toString() + ':' + block.kind)
      })
    }.alignItems(HorizontalAlign.Start).width('100%')
  }
}
