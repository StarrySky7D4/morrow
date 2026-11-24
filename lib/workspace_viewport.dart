import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart' show listEquals;
import 'appearance.dart';
import 'stable_masonry_grid.dart';
import 'package:flutter_staggered_grid_view/flutter_staggered_grid_view.dart';
import 'ui/workbench_motion.dart';

/// One viewport, bounded prefetch, stable placement identities. Only the current
/// page is mounted; five scroll offsets are retained, never card resources.
class WorkspaceViewport extends StatefulWidget {
  const WorkspaceViewport({
    super.key,
    required this.page,
    this.pageIndex = 0,
    required this.ids,
    required this.header,
    required this.footer,
    required this.itemBuilder,
    required this.twoColumnWidth,
    this.threeColumnWidth = 1080,
    this.compactList = false,
    this.status,
    this.cardRegionKey,
    this.queryPending = false,
    this.session,
  });
  final String page;
  final int pageIndex;
  final List<String> ids;
  final Widget header, footer;
  final Widget? status;
  final Key? cardRegionKey;
  final bool queryPending;
  final Object? session;
  final IndexedWidgetBuilder itemBuilder;
  final double twoColumnWidth;
  final double threeColumnWidth;
  final bool compactList;

  @override
  State<WorkspaceViewport> createState() => _WorkspaceViewportState();
}

class _WorkspaceViewportState extends State<WorkspaceViewport>
    with SingleTickerProviderStateMixin {
  final _scroll = ScrollController(keepScrollOffset: false);
  final _offsets = <String, double>{};
  double? _restoreOffset;
  List<String>? _indexedIds;
  String? _indexedPage;
  Map<Key, int> _indexes = {};
  List<String> _retainedIds = const [];
  IndexedWidgetBuilder? _retainedBuilder;
  double _direction = 1.0;
  late final _reveal = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 280),
    value: 1,
  );
  late final CurvedAnimation _curvedReveal = CurvedAnimation(
    parent: _reveal,
    curve: WorkbenchCurves.springOut,
  );

  @override
  void didUpdateWidget(WorkspaceViewport oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.page != widget.page ||
        !identical(oldWidget.session, widget.session)) {
      _retainedIds = const [];
      _retainedBuilder = null;
    } else if (widget.queryPending &&
        !oldWidget.queryPending &&
        _scroll.hasClients) {
      _restoreOffset = _scroll.offset;
    }
    if (oldWidget.page != widget.page) {
      _direction = widget.pageIndex >= oldWidget.pageIndex ? 1.0 : -1.0;
      if (_scroll.hasClients) {
        _offsets.remove(oldWidget.page);
        _offsets[oldWidget.page] = _scroll.offset;
      }
      _restoreOffset = _offsets[widget.page] ?? 0;
      while (_offsets.length > 5) {
        _offsets.remove(_offsets.keys.first);
      }
      if (MediaQuery.disableAnimationsOf(context)) {
        _reveal.value = 1;
      } else {
        _reveal.forward(from: 0);
      }
    } else if (oldWidget.compactList != widget.compactList) {
      _direction = 1.0;
      if (MediaQuery.disableAnimationsOf(context)) {
        _reveal.value = 1;
      } else {
        _reveal.forward(from: 0);
      }
    }
  }

  @override
  void dispose() {
    _curvedReveal.dispose();
    _scroll.dispose();
    _reveal.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Preserve only the attached, virtualized card views while the host checks
    // a new result. Hidden old data cannot paint, receive input or run tickers.
    // A failure/empty result clears it; it is never used as an authorized result.
    if (!widget.queryPending) {
      _retainedIds = widget.status == null ? widget.ids : const [];
      _retainedBuilder = widget.status == null ? widget.itemBuilder : null;
    }
    final ids = widget.queryPending ? _retainedIds : widget.ids;
    final itemBuilder = widget.queryPending
        ? _retainedBuilder
        : widget.itemBuilder;
    // Masonry must establish its first row before restoring a nonzero offset.
    // Wait for a real result as a loading panel cannot represent its extent.
    if (_restoreOffset != null && widget.status == null) {
      final page = widget.page;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || page != widget.page || !_scroll.hasClients) return;
        final offset = _restoreOffset;
        _restoreOffset = null;
        if (offset != null) {
          _scroll.jumpTo(offset.clamp(0, _scroll.position.maxScrollExtent));
        }
      });
    }
    if (_indexedPage != widget.page || !listEquals(_indexedIds, ids)) {
      _indexedIds = ids;
      _indexedPage = widget.page;
      _indexes = {
        for (var i = 0; i < ids.length; i++) ValueKey((widget.page, ids[i])): i,
      };
    }
    final headerMorphDuration = MediaQuery.disableAnimationsOf(context)
        ? Duration.zero
        : const Duration(milliseconds: 260);
    final transparentBackdrop =
        AppearanceScope.maybeOf(context)?.backdrop ==
        BackgroundMode.transparent;
    final veilColor = Theme.of(context).colorScheme.surface;
    final scrollView = KeyedSubtree(
      key: ValueKey('page-${widget.page}'),
      child: CustomScrollView(
        controller: _scroll,
        key: PageStorageKey('workspace-scroll-${widget.page}'),
        scrollCacheExtent: const ScrollCacheExtent.pixels(300),
        slivers: [
          SliverToBoxAdapter(
            child: AnimatedSize(
              duration: headerMorphDuration,
              curve: WorkbenchCurves.springOut,
              alignment: Alignment.topCenter,
              child: widget.header,
            ),
          ),
          SliverToBoxAdapter(
            child: widget.status ?? const SizedBox.shrink(),
          ),
          ExcludeFocus(
            key: ValueKey(widget.session),
            excluding: widget.status != null,
            child: SliverVisibility(
              visible: widget.status == null,
              maintainState: true,
              sliver: SliverLayoutBuilder(
                key: widget.cardRegionKey,
                builder: (context, constraints) {
                  final extent = constraints.crossAxisExtent;
                  final crossAxisCount = widget.compactList
                      ? 1
                      : (widget.twoColumnWidth.isFinite &&
                                extent >= widget.threeColumnWidth)
                      ? 3
                      : extent >= widget.twoColumnWidth
                      ? 2
                      : 1;
                  return StableMasonryGrid(
                    ids: ids,
                    gridDelegate:
                        SliverSimpleGridDelegateWithFixedCrossAxisCount(
                          crossAxisCount: crossAxisCount,
                        ),
                    mainAxisSpacing: widget.compactList ? 8 : 14,
                    crossAxisSpacing: 14,
                    delegate: SliverChildBuilderDelegate(
                      (context, index) => KeyedSubtree(
                        key: ValueKey((widget.page, ids[index])),
                        child: itemBuilder!(context, index),
                      ),
                      childCount: itemBuilder == null ? 0 : ids.length,
                      findChildIndexCallback: (key) => _indexes[key],
                      // Cards have no inline editor; drafts live in editor sessions.
                      addAutomaticKeepAlives: false,
                    ),
                  );
                },
              ),
            ),
          ),
          SliverToBoxAdapter(child: widget.footer),
        ],
      ),
    );
    return SlideTransition(
      position: Tween<Offset>(
        begin: Offset(0, _direction * 0.024),
        end: Offset.zero,
      ).animate(_curvedReveal),
      child: ScaleTransition(
        scale: Tween<double>(begin: 0.988, end: 1.0).animate(_curvedReveal),
        child: AnimatedBuilder(
          animation: _reveal,
          child: scrollView,
          builder: (context, child) {
            final animating = _reveal.isAnimating;
            final fade = animating
                ? Curves.easeOutCubic.transform(_reveal.value)
                : 1.0;
            return Stack(
              fit: StackFit.passthrough,
              children: [
                child!,
                if (animating && !transparentBackdrop)
                  Positioned.fill(
                    child: IgnorePointer(
                      child: ColoredBox(
                        color: veilColor.withValues(alpha: 0.28 * (1.0 - fade)),
                      ),
                    ),
                  ),
              ],
            );
          },
        ),
      ),
    );
  }
}
