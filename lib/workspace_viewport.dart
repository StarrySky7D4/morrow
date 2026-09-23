import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart' show listEquals;
import 'stable_masonry_grid.dart';
import 'package:flutter/rendering.dart' show ScrollCacheExtent;
import 'package:flutter_staggered_grid_view/flutter_staggered_grid_view.dart';

/// One viewport, bounded prefetch, stable placement identities. Only the current
/// page is mounted; five scroll offsets are retained, never card resources.
class WorkspaceViewport extends StatefulWidget {
  const WorkspaceViewport({
    super.key,
    required this.page,
    required this.ids,
    required this.header,
    required this.footer,
    required this.itemBuilder,
    required this.twoColumnWidth,
    this.status,
    this.cardRegionKey,
    this.queryPending = false,
    this.session,
  });
  final String page;
  final List<String> ids;
  final Widget header, footer;
  final Widget? status;
  final Key? cardRegionKey;
  final bool queryPending;
  final Object? session;
  final IndexedWidgetBuilder itemBuilder;
  final double twoColumnWidth;

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
  late final _reveal = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
    value: 1,
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
    }
  }

  @override
  void dispose() {
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
    return FadeTransition(
      opacity: _reveal,
      child: KeyedSubtree(
        key: ValueKey('page-${widget.page}'),
        child: CustomScrollView(
          controller: _scroll,
          key: PageStorageKey('workspace-scroll-${widget.page}'),
          scrollCacheExtent: const ScrollCacheExtent.pixels(300),
          slivers: [
            SliverToBoxAdapter(child: widget.header),
            SliverToBoxAdapter(child: widget.status ?? const SizedBox.shrink()),
            ExcludeFocus(
              key: ValueKey(widget.session),
              excluding: widget.status != null,
              child: SliverVisibility(
                visible: widget.status == null,
                maintainState: true,
                sliver: SliverLayoutBuilder(
                  key: widget.cardRegionKey,
                  builder: (context, constraints) => StableMasonryGrid(
                    gridDelegate:
                        SliverSimpleGridDelegateWithFixedCrossAxisCount(
                          crossAxisCount:
                              constraints.crossAxisExtent >=
                                  widget.twoColumnWidth
                              ? 2
                              : 1,
                        ),
                    mainAxisSpacing: 14,
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
                  ),
                ),
              ),
            ),
            SliverToBoxAdapter(child: widget.footer),
          ],
        ),
      ),
    );
  }
}
