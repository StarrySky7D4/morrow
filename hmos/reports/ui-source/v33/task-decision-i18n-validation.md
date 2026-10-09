# v33 待确认任务文案九语同步

2026-10-09：使用实际 Flutter 设计文案接通新 HMOS 待确认任务菜单、两个明确选择按钮及说明；协调 task agent 同步 Index 的错误提示与读屏标签。该 agent 拥有 Index 和业务测试，本项没有代改它的文件。

三条任务设计源为 `mainTaskMarkComplete`、`mainTaskMarkIncomplete`、`mainTaskAmbiguousDecision`。原新写短语“标记完成 / 标记未完成 / 待确认，请明确选择完成或未完成”没有相同 ARB 源；采用已有源中文“确认已完成 / 确认未完成 / 旧版同名待办的完成状态无法确定，请分别确认。”，避免另造语义偏离的八语别名。

这三条源文案已经完整存在于当前 `UiStrings.ets` 八个非中文词典。中文遵循该模型既有原文返回行为；其余八语严格核对真实条目存在并匹配源翻译，不能依靠中文 fallback。UiStrings没有发生字节变化，没有重新生成或改动无关译文。

| 语言 | 明确完成 | 明确未完成 | 待确认说明 |
| --- | --- | --- | --- |
| zh | 确认已完成 | 确认未完成 | 旧版同名待办的完成状态无法确定，请分别确认。 |
| en | Confirm complete | Confirm incomplete | The completion of these legacy tasks with matching names is uncertain. Confirm each task separately. |
| ja | 完了として確定 | 未完了として確定 | 旧形式の同名タスクの完了状態は不明です。各タスクを個別に確認してください。 |
| ko | 완료로 확인 | 미완료로 확인 | 이름이 같은 이전 형식 작업의 완료 상태가 불확실합니다. 각 작업을 개별적으로 확인하세요. |
| de | Als erledigt bestätigen | Als offen bestätigen | Der Status dieser alten Aufgaben mit gleichem Namen ist unklar. Bestätigen Sie jede Aufgabe einzeln. |
| fr | Confirmer : terminée | Confirmer : non terminée | L’état de ces anciennes tâches de même nom est incertain. Confirmez chaque tâche séparément. |
| es | Confirmar completada | Confirmar pendiente | El estado de estas tareas antiguas con el mismo nombre es incierto. Confirma cada tarea por separado. |
| pt | Confirmar concluída | Confirmar pendente | O estado destas tarefas antigas com o mesmo nome é incerto. Confirme cada tarefa separadamente. |
| ru | Подтвердить выполнение | Подтвердить невыполнение | Состояние этих старых задач с одинаковыми названиями неизвестно. Подтвердите каждую задачу отдельно. |

## 可重复核对

- `hmos/tool/task-decision-i18n.cjs`：读取 `build/win-cloud-20261005/l10n/parts/main.<locale>.arb` 与 `packages/morrow_i18n/lib/l10n/app_<locale>.arb`，逐条核对源 parts 和整合 ARB 一致；解析真实 UiStrings，并只去掉两个 ArkTS类型声明后执行真实 `uiText` 函数体。
- `hmos/tool/generate-ui-strings.cjs --check-task-decisions`：只读核对这三个源设计短语、九语和实际27次查找。保留原全集生成器默认行为；本项没有运行全量生成，以免当前历史 `io-safety-refactor` 全集引用引入无关目录差异。
- `hmos/tool/task-decision-i18n.test.cjs`：九个真实语言场景、实际 Index 文案入口及缺失旧短语检查、用户文字与固定身份不翻译、只读生成器核对不改变任何 UiStrings 字节。
- `hmos/tool/run-task-decision-i18n.cjs <new-label>`：冻结24项实际输入、使用明确TAP输出执行测试、再次核对全部输入、保留单独日志与结果。标签文件已经存在时拒绝覆盖。

## 本轮结果

`task-decision-i18n-a2-result.json`：**12 / 12 PASS**，0失败、跳过或取消；24项源输入前后一致，包含18份实际Flutter ARB、真实UiStrings、Index及本项工具。执行230 ms；实际生产 `uiText` 的27项源短语查找与九语设计译文逐字一致。

日志 `task-decision-i18n-a2-tests.log` SHA256：`0ec0186cc1ac7994926bd2e5e1c5382dcf1e7fed75842ded8d795f898f157135`。Node v24.14.1，执行文件SHA256：`58e74bf02fc5bbacc41dcb8bef089961cd5bddd37830b87784e4fc624d145d1f`。

历史 a1 结果保留：测试本身12项通过、退出0且24输入稳定，但Node默认spec输出没有TAP `# tests` 行，旧runner无法确认计数而标为 qualification FAIL。修正runner明确指定TAP后独立执行a2；不把a1未合格结果改写成通过。

本项新增/改动范围仅本项工具与报告；UiStrings保持SHA256 `17801dd05b1afd9684f452b0e5b736ffba7c5520fb5413a8b6d002216cb1a576`。task agent完成源文案接入后继续实现独立 guards，a2明确对应其采样时的Index快照；root最终冻结后的整包校验应重新执行本项或包含该 test，不能把较早整个Index的hash当作后续未修改证明。

SDK构建、ArkUI实际排版、语言长度/窄屏、设备读屏及实际业务提交均 **NOT_RUN**。这份检查证明真实源文案/实际翻译查找和源入口，不能声称字体渲染、所有页面或九语设备UI已经通过。
