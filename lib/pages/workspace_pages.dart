part of '../main.dart';

extension _PageContent on _StudioState {
  List<WorkbenchFilter> get pageFilters => section.filters;
  String projectStage(Idea idea) =>
      idea.todos.isNotEmpty && idea.completed.length >= idea.todos.length
      ? '已完成'
      : ['计划中', '推进中', '已完成'].contains(idea.stage)
      ? idea.stage
      : '推进中';
  bool matchesPageFilter(Idea idea) => switch (filter) {
    GeneralFilter.all => true,
    GeneralFilter.pendingTodos => idea.todos.any(
      (todo) => !idea.completed.contains(todo),
    ),
    GeneralFilter.attachments => idea.attachments.isNotEmpty,
    GeneralFilter.favorites => idea.favorite,
    GeneralFilter.image => idea.attachments.any(
      (a) => [TextureKind.image, TextureKind.gif].contains(a.source.kind),
    ),
    GeneralFilter.media => idea.attachments.any(
      (a) => [TextureKind.video, TextureKind.audio].contains(a.source.kind),
    ),
    GeneralFilter.file => idea.attachments.any(
      (a) => a.source.kind == TextureKind.file,
    ),
    GeneralFilter.text => idea.attachments.isEmpty,
    StageFilter(:final stage) =>
      (section == WorkbenchPage.projects ? projectStage(idea) : idea.stage) ==
          WorkbenchV1.stage(stage),
  };
  void changeStage(Idea idea, String stage) {
    if (widget.workbench != null) {
      pluginChange(PluginAction.stage, idea, text: stage);
      return;
    }
    refreshPage(() {
      idea.stage = stage;
      if (idea.category == '进行中') {
        if (stage == '已完成') {
          idea.completed.addAll(idea.todos);
        } else if (idea.todos.isNotEmpty &&
            idea.completed.length == idea.todos.length) {
          idea.completed.remove(idea.todos.last);
        }
      }
    });
    persist();
  }

  void moveToProject(Idea idea) {
    if (widget.workbench != null) {
      pluginChange(PluginAction.toProject, idea);
      return;
    }
    refreshPage(() {
      idea.category = '进行中';
      idea.stage = '计划中';
    });
    persist();
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(l.mainMovedProject(displayTitle(idea))),
        action: SnackBarAction(
          label: l.mainView,
          onPressed: () => selectSection(WorkbenchPage.projects),
        ),
      ),
    );
  }

  Widget stageMenu(Idea idea, List<String> stages) => PopupMenuButton<String>(
    tooltip: l.mainStageTooltip(displayTitle(idea)),
    onSelected: (value) => changeStage(idea, value),
    itemBuilder: (_) => stages
        .map(
          (stage) =>
              PopupMenuItem(value: stage, child: Text(stageLabel(stage))),
        )
        .toList(),
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            stageLabel(
              idea.category == '进行中' ? projectStage(idea) : idea.stage,
            ),
            style: TextStyle(color: p.accent, fontSize: 11),
          ),
          Icon(Icons.expand_more, size: 16, color: p.accent),
        ],
      ),
    ),
  );
  Widget bookmark(Idea idea) => IconButton(
    tooltip: idea.favorite
        ? l.mainUnfavoriteTooltip(displayTitle(idea))
        : l.mainFavoriteTooltip(displayTitle(idea)),
    onPressed: () {
      if (widget.workbench != null) {
        pluginChange(PluginAction.favorite, idea, flag: !idea.favorite);
        return;
      }
      refreshPage(() => idea.favorite = !idea.favorite);
      persist();
    },
    icon: Icon(
      idea.favorite ? Icons.bookmark_rounded : Icons.bookmark_border_rounded,
      size: 18,
      color: p.accent,
    ),
  );
  Widget recordTitle(Idea idea) => Text(
    displayTitle(idea),
    maxLines: 2,
    overflow: TextOverflow.ellipsis,
    style: TextStyle(fontSize: 15, fontWeight: FontWeight.w600, color: p.ink),
  );
  Widget recordSummary(Idea idea) => Padding(
    padding: const EdgeInsets.only(top: 8),
    child: Text(
      displayDescription(idea),
      maxLines: 3,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(fontSize: 12, height: 1.7, color: p.muted),
    ),
  );
  Widget attachmentHint(Idea idea) => idea.attachments.isEmpty
      ? const SizedBox.shrink()
      : Padding(
          padding: const EdgeInsets.only(top: 12),
          child: Row(
            children: [
              Icon(Icons.attach_file, size: 14, color: p.accent),
              const SizedBox(width: 4),
              Expanded(
                child: Text(
                  l.mainAttachmentHint(
                    idea.attachments.length,
                    idea.attachments.first.source.name,
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 10, color: p.accent),
                ),
              ),
            ],
          ),
        );
  Widget recordShell(Idea idea, Widget child, {Key? key}) => Glass(
    componentId: 'card:${idea.id}',
    key: key,
    p: p,
    radius: 20,
    child: Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: () => openIdea(idea),
        borderRadius: p.borderRadius(20),
        child: Padding(padding: const EdgeInsets.all(18), child: child),
      ),
    ),
  );

  Widget specializedCards(List<Idea> items) => switch (section) {
    WorkbenchPage.inbox => Column(
      key: const ValueKey('inbox-list'),
      children: items
          .map(
            (idea) => Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: recordShell(
                idea,
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(Icons.inbox_outlined, size: 19, color: p.accent),
                        const SizedBox(width: 12),
                        Expanded(child: recordTitle(idea)),
                        bookmark(idea),
                      ],
                    ),
                    recordSummary(idea),
                    attachmentHint(idea),
                    const SizedBox(height: 12),
                    Wrap(
                      spacing: 8,
                      children: [
                        TextButton.icon(
                          onPressed: () => changeStage(
                            idea,
                            idea.stage == '已整理' ? '待整理' : '已整理',
                          ),
                          icon: Icon(
                            idea.stage == '已整理'
                                ? Icons.check_circle
                                : Icons.circle_outlined,
                            size: 16,
                          ),
                          label: Text(
                            idea.stage == '已整理'
                                ? l.mainStageOrganized
                                : l.mainMarkOrganized,
                          ),
                        ),
                        OutlinedButton.icon(
                          key: ValueKey('promote-${idea.id}'),
                          onPressed: () => moveToProject(idea),
                          icon: const Icon(Icons.arrow_forward, size: 16),
                          label: Text(l.mainToProject),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          )
          .toList(),
    ),
    WorkbenchPage.projects => LayoutBuilder(
      builder: (_, constraints) {
        final columns = constraints.maxWidth >= 660 ? 2 : 1;
        return Wrap(
          key: const ValueKey('project-board'),
          spacing: 14,
          runSpacing: 14,
          children: items
              .map(
                (idea) => SizedBox(
                  width: (constraints.maxWidth - (columns - 1) * 14) / columns,
                  child: recordShell(
                    idea,
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Icon(Icons.folder_open, color: p.accent, size: 20),
                            const SizedBox(width: 10),
                            Expanded(child: recordTitle(idea)),
                            bookmark(idea),
                          ],
                        ),
                        recordSummary(idea),
                        const SizedBox(height: 12),
                        Row(
                          children: [
                            stageMenu(idea, ['计划中', '推进中', '已完成']),
                            const Spacer(),
                            Text(
                              l.mainSteps(
                                idea.completed.length,
                                idea.todos.length,
                              ),
                              style: TextStyle(fontSize: 11, color: p.muted),
                            ),
                          ],
                        ),
                        ClipRRect(
                          borderRadius: p.borderRadius(8),
                          child: LinearProgressIndicator(
                            value: idea.todos.isEmpty
                                ? (projectStage(idea) == '已完成' ? 1 : 0)
                                : idea.completed.length / idea.todos.length,
                            minHeight: 5,
                            backgroundColor: p.line,
                          ),
                        ),
                        const SizedBox(height: 12),
                        if (idea.todos.isEmpty)
                          TextButton(
                            onPressed: () => openIdea(idea),
                            child: Text(l.mainOpenNextStep),
                          ),
                        ...idea.todos
                            .take(3)
                            .map(
                              (todo) => LittleTask(
                                title: displayTodo(idea, todo),
                                done: idea.completed.contains(todo),
                                onChanged: (done) {
                                  if (widget.workbench != null) {
                                    pluginChange(
                                      PluginAction.todo,
                                      idea,
                                      text: todo,
                                      flag: done,
                                    );
                                    return;
                                  }
                                  refreshPage(() {
                                    if (done) {
                                      idea.completed.add(todo);
                                    } else {
                                      idea.completed.remove(todo);
                                      idea.stage = '推进中';
                                    }
                                  });
                                  persist();
                                },
                              ),
                            ),
                        if (idea.todos.length > 3)
                          Text(
                            l.mainMoreSteps(idea.todos.length - 3),
                            style: TextStyle(fontSize: 10, color: p.muted),
                          ),
                        attachmentHint(idea),
                      ],
                    ),
                  ),
                ),
              )
              .toList(),
        );
      },
    ),
    WorkbenchPage.laboratory => Column(
      key: const ValueKey('experiment-journal'),
      children: items.asMap().entries.map((entry) {
        final idea = entry.value;
        return Padding(
          padding: const EdgeInsets.only(bottom: 16),
          child: recordShell(
            idea,
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Text(
                      'EXP ${(entry.key + 1).toString().padLeft(2, '0')}',
                      style: TextStyle(
                        letterSpacing: 2,
                        color: p.accent,
                        fontSize: 11,
                      ),
                    ),
                    const Spacer(),
                    stageMenu(idea, ['待验证', '验证中', '已记录']),
                    bookmark(idea),
                  ],
                ),
                recordTitle(idea),
                recordSummary(idea),
                Divider(height: 28, color: p.line),
                Text(
                  l.mainHypothesisSection,
                  style: TextStyle(color: p.accent, fontSize: 10),
                ),
                const SizedBox(height: 6),
                Text(
                  idea.hypothesis.isEmpty
                      ? l.mainWriteHypothesis
                      : idea.hypothesis,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: p.ink, height: 1.7, fontSize: 12),
                ),
                const SizedBox(height: 16),
                Text(
                  l.mainObservationSection,
                  style: TextStyle(color: p.accent, fontSize: 10),
                ),
                const SizedBox(height: 6),
                Text(
                  idea.conclusion.isEmpty ? l.mainNoResultYet : idea.conclusion,
                  maxLines: 4,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: p.muted, height: 1.7, fontSize: 12),
                ),
                attachmentHint(idea),
              ],
            ),
          ),
        );
      }).toList(),
    ),
    _ => Column(
      key: const ValueKey('favorites-library'),
      children: items
          .map(
            (idea) => Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: recordShell(
                idea,
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SizedBox(
                      width: 56,
                      height: 56,
                      child: ClipRRect(
                        borderRadius: p.borderRadius(12),
                        child:
                            idea.attachments.any(
                              (a) => [
                                TextureKind.image,
                                TextureKind.gif,
                              ].contains(a.source.kind),
                            )
                            ? TrackCover(
                                source: idea.attachments
                                    .firstWhere(
                                      (a) => [
                                        TextureKind.image,
                                        TextureKind.gif,
                                      ].contains(a.source.kind),
                                    )
                                    .source,
                              )
                            : ColoredBox(
                                color: p.accent.withValues(alpha: .12),
                                child: Icon(
                                  idea.attachments.isEmpty
                                      ? Icons.notes
                                      : Icons.folder_copy_outlined,
                                  color: p.accent,
                                ),
                              ),
                      ),
                    ),
                    const SizedBox(width: 14),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          recordTitle(idea),
                          recordSummary(idea),
                          attachmentHint(idea),
                        ],
                      ),
                    ),
                    bookmark(idea),
                  ],
                ),
              ),
            ),
          )
          .toList(),
    ),
  };

  Widget pageIntro() {
    final (icon, text, stats) = switch (section) {
      WorkbenchPage.inbox => (
        Icons.inbox_outlined,
        l.mainInboxIntro,
        <(String, int)>[
          (
            l.mainUnsortedCount,
            ideas.where((i) => i.category == '灵感' && i.stage != '已整理').length,
          ),
          (
            l.mainOrganizedCount,
            ideas.where((i) => i.category == '灵感' && i.stage == '已整理').length,
          ),
        ],
      ),
      WorkbenchPage.projects => (
        Icons.folder_open,
        l.mainProjectIntro,
        <(String, int)>[
          (
            l.mainActiveProjects,
            ideas
                .where((i) => i.category == '进行中' && projectStage(i) != '已完成')
                .length,
          ),
          (
            l.mainCompletedProjects,
            ideas
                .where((i) => i.category == '进行中' && projectStage(i) == '已完成')
                .length,
          ),
        ],
      ),
      WorkbenchPage.laboratory => (
        Icons.science_outlined,
        l.mainLabIntro,
        <(String, int)>[
          (
            l.mainUnverifiedCount,
            ideas.where((i) => i.category == '实验' && i.stage == '待验证').length,
          ),
          (
            l.mainRecordedCount,
            ideas
                .where((i) => i.category == '实验' && i.conclusion.isNotEmpty)
                .length,
          ),
        ],
      ),
      _ => (
        Icons.bookmarks_outlined,
        l.mainFavoritesIntro,
        <(String, int)>[
          (l.mainFavoriteRecords, ideas.where((i) => i.favorite).length),
          (
            l.mainFavoriteAttachments,
            ideas
                .where((i) => i.favorite)
                .fold<int>(0, (n, i) => n + i.attachments.length),
          ),
        ],
      ),
    };
    return Glass(
      componentId: WorkbenchV1.summaryComponentId(section),
      p: p,
      child: Padding(
        padding: const EdgeInsets.all(22),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(icon, color: p.accent, size: 28),
                const SizedBox(width: 16),
                Expanded(
                  child: Text(
                    text,
                    style: TextStyle(color: p.muted, height: 1.8, fontSize: 12),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 18),
            Wrap(
              spacing: 24,
              runSpacing: 10,
              children: stats
                  .map(
                    (stat) => Text(
                      '${stat.$1}  ${stat.$2}',
                      style: TextStyle(color: p.ink, fontSize: 12),
                    ),
                  )
                  .toList(),
            ),
          ],
        ),
      ),
    );
  }
}
