"""One-time evidence inventory and diagnostic matrix, not a wire contract."""
import sys
sys.dont_write_bytecode = True
from check_review import HERE, JOINT, HOST, PLUGIN, read, write, sha, verify_pins

def main():
    if (HERE / 'baseline.json').exists() or (HERE / 'matrix.json').exists():
        raise RuntimeError('refuse to replace an existing batch baseline/matrix')
    paths = {}
    def pin(path, expected=None, scope='frozen_previous_stage'):
        path = path.resolve()
        expected = expected or sha(path)
        key = str(path)
        if key in paths and paths[key]['sha256'] != expected:
            raise ValueError('conflicting expected identity: ' + key)
        paths[key] = {'path': key, 'sha256': expected, 'scope': scope}
    snapshot_path = JOINT / 'runs/round6-final-gate/snapshot.json'
    snapshot = read(snapshot_path)
    pin(snapshot_path)
    for item in snapshot['plan_files']:
        pin(__import__('pathlib').Path(item['path']), item['expected'], 'original_plan')
    for item in snapshot['frozen_boundary_files']:
        pin(HOST / item['path'], item['disk_sha256'], 'legacy_contract_and_wire')
    manifest_path = HOST / 'reports/codex-morrow-v1.1/host/host-kit-003/manifest.json'
    pin(manifest_path, '5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01')
    kit = read(manifest_path)
    for item in kit['files']:
        pin(manifest_path.parent / item['path'], item['sha256'], 'kit003')
    for item in kit['source_files']:
        pin(__import__('pathlib').Path(kit['authority']) / item['path'], item['sha256'], 'canonical_v1')
    prior_path = JOINT / 'runs/p02-integration-004-replay-001/inputs-after.json'
    pin(prior_path)
    for relative, expected in read(prior_path).items():
        pin(PLUGIN / relative, expected, 'previous_plugin_handoffs')
    for name in ['index.json', 'coverage.csv', 'g0-checklist.json', 'README.md']:
        pin(JOINT / name, scope='previous_joint_ledger')
    for path in list(JOINT.glob('*.py')) + list(JOINT.glob('review-*.md')):
        pin(path, scope='previous_joint_tool_or_report')
    pin(HOST / 'workbench_host/src/main.rs', scope='legacy_UI_stdio_entrypoint_source_only')
    pin(JOINT / 'runs/p02-integration-004-replay-001/result.json', 'f9a2621816f6b71ae722f68d08a5527008883eabf6435f1040e8f5040ddb6a8f')
    _, issues = verify_pins(list(paths.values()))
    if issues:
        raise ValueError(issues)
    cat = read(JOINT / 'index.json')
    diagnostic_specs = [
      ('宿主准入与实际子进程绑定', ['NX-01'], '独立核对启动exe摘要、固定argv/cwd/env策略/平台、父子PID、实际通道与宿主创建的实例代次；审共享准入代码', '客户端自报artifact/pluginId/approved不能创建权限；未证明可信安装/防换包或OS强隔离'),
      ('真实握手和只读状态', ['NX-01','NX-02'], '保存真实双向帧原字节/长度/摘要、宿主会话状态、关联请求与host/child PID；帧按唯一宿主合同独立解码', '只读状态成功不计网络/执行/存储业务成功，认证/业务能力明确unsupported'),
      ('伪造准入与绑定字段', ['NX-01','P01'], '真实客户端分别伪造approved/身份/摘要/平台/协议或请求能力；审不可由自报升级的宿主状态并观察明确拒绝', '拒绝不能隐式fallback；未覆盖字段逐项not_run'),
      ('旧实例和旧代次', ['NX-02','P01'], '新一轮真实实例发送已关闭或撤销实例的代次/连接数据；独立关联新旧PID和帧', '拒绝旧代次，不能通过重连恢复权限；不能将测试代次替代正式CLI认证'),
      ('在线撤权和在途交付', ['P01','NX-04'], '记录发送、宿主撤权、结果交付的单调顺序与权限代次；覆盖撤权后的新查询及可控在途结果', '新操作及失效结果拒绝，已发生事实保留；若无在途窗口则明确not_run'),
      ('首次准入到期不续期', ['P01'], '实际延迟/重试跨过首次准入创建的单调截止时间；记录初次deadline与所有重试结果', '任何重试/握手/查询不能重置审批期限；墙钟时间不作期限依据'),
      ('控制路径独立且有界', ['NX-05'], '真实业务/无效帧饱和期间发出停止或撤权；记录限额、控制到达、处理和退出的单调时差', '不能仅静态声明双队列；不允许正文或畸形队列长期饿死控制，观察不足不通过'),
      ('畸形帧与读写超时', ['NX-05','NX-06'], '真实截断/超长/错误版本帧或客户端不响应；观察拒绝、停止、有限线程等待、EOF', '长度界限在分配前验证；超时不重用旧成功或无限join'),
      ('正常退出与输出关闭分开', ['NX-06','B05'], '父宿主实际等待child并记录退出码；联合独立观察PID终止、stdout/stderr EOF及保留尾部输出或gap', '退出事件本身不代表输出关闭或业务成功'),
      ('ClosingUnconfirmed保留所有权', ['NX-06','B05'], '针对无法确认关闭的受控情形观察ClosingUnconfirmed/owner留存、后续准入拒绝及最终回收；区分实际故障与注入观察', '无法确认就不能释放owner或启动第二writer，注入观测不能冒充真实进程树隔离'),
      ('断连与未知未决', ['NX-02','NX-04','P01'], '真实通道断开后查询/交付拒绝；记录未决操作身份，确认只核对而不创建新操作重试', '首片无writer/业务执行，因此其未知业务恢复仍not_run'),
      ('共享实现与fixture边界', ['QX-03'], '追踪真实host入口至可复用会话/审批/timeout所有权模块；检查临时fixture准入受限入口及生产构建依赖', '测试特供第二授权体系、fixture泄漏普通生产入口不得通过；本片不是安装版产品'),
      ('旧合同与UI入口冻结', ['L06'], '003 kit、v1 canonical、旧UI stdio入口和旧边界文件逐字节核验，旧handoff/工具/回执保持原摘要', '静态身份一致不替代旧UI或guest运行兼容性回归'),
      ('可重放证据与失败闭合', ['QX-01','QX-02'], '新源码/锁/构建输入/exe/原始帧/运行目录关联；独立观察超时与非零失败，未复用旧成功', '本地临时host和受控client不证明可信安装、平台全覆盖或生产OS隔离'),
    ]
    cases = [{'id': f'M02D-{n:02}', 'title': title, 'related_original_ids': refs,
              'required_observations': need, 'rejection_or_limit': limit,
              'status': 'not_run', 'product_pass_credit': 0,
              'mapping_basis': 'reviewer diagnostic relevance, not original scenario completion'}
             for n,(title,refs,need,limit) in enumerate(diagnostic_specs, 1)]
    ids = {i for c in cases for i in c['related_original_ids']}
    anchors = [{k:i[k] for k in ['id','scenario','must_prove','source']} for i in cat['acceptance'] if i['id'] in ids]
    matrix = {'kind':'joint_diagnostic_matrix_not_wire_schema', 'batch':'m02-native-session-001',
      'dependencies':[{'owner':'host','input':'new authoritative versioned contract, kit, fixed vectors, reusable session module, safe fixture launch CLI','state':'awaiting_ready'},
                      {'owner':'plugin','input':'client consuming exact host contract, new lock/build/exe identity, controlled adversarial modes','state':'awaiting_ready'},
                      {'owner':'joint','input':'source and identity audit before independent real host/client runs','state':'preflight_only'}],
      'work_package_source':next(i['source'] for i in cat['work_packages'] if i['id']=='M-02'),
      'original_acceptance':anchors,'diagnostics':cases,
      'product_status':{'M-02':'not_run','P-02':'blocked','J-00':'blocked','G0':'blocked','G1':'not_run','graphs':'0/2','acceptance_not_run':84},
      'contract_adapter':'pending_host_authority_no_local_schema',
      'excluded':['success HTTP/SSE/WS','durable writer','arbitrary B commands','real accounts and secrets','full trusted installation','OS strong isolation','other platform runtime qualification']}
    write(HERE / 'baseline.json', {'kind':'reviewer_frozen_file_inventory','files':list(paths.values())})
    write(HERE / 'matrix.json', matrix)
    print(f'Frozen {len(paths)} files; {len(cases)} diagnostic cases, 0 executed; product 84 not_run unchanged.')

if __name__ == '__main__':
    main()
