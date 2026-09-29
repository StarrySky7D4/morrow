"""Maintain this review's status ledger from specific observed evidence."""
import json
import sys
from datetime import datetime, timezone
sys.dont_write_bytecode=True
from check_joint import HERE, HOST, PLAN, digest, write_json, export_csv

def evidence(relative, scope):
    path=HERE/relative
    return {'path':str(path),'sha256':digest(path.read_bytes()),'scope':scope}

def main():
    checks=[
      {'id':'G0-C01','requirement':'31工作包、G0–G6、84验收与W01–W28原编号及来源一致','status':'verified',
       'evidence':[evidence('runs/source-bound-index-002/result.json','catalogue_integrity')], 'limit':'不证明任何产品场景通过'},
      {'id':'G0-C02','requirement':'M-00基线、输入摘要、能力台账与本地前置','status':'verified',
       'evidence':[evidence('runs/m00-handoff-001/result.json','baseline_handoff')], 'limit':'178输入与7计划/校验文件复核；运行目标存在不证明平台资格'},
      {'id':'G0-C03','requirement':'旧Wasm/guest合同、原件与旧wire保留','status':'verified',
       'evidence':[evidence('runs/m00-handoff-001/result.json','frozen_original_identity')], 'limit':'36固定文件/13原件对与SDK副本；未执行旧guest运行兼容回归'},
      {'id':'G0-C04','requirement':'唯一宿主Schema与版本化新wire、kit、向量、失败回执一致','status':'blocked','evidence':[],
       'limit':'宿主唯一Schema路径已指定，正在实现；完整kit清单未交接，尚未执行字节/fake复核'},
      {'id':'G0-C05','requirement':'插件native/Wasm两构建图、锁与缺依赖失败传播','status':'blocked','evidence':[],
       'limit':'入口已实现并收到43负例回执，但三个当前输入摘要已变化；两图仍未实现，等待最终源码与回执对齐'},
      {'id':'G0-C06','requirement':'固定Codex/CC Switch源码原件、补丁和候选闭包审查','status':'blocked','evidence':[],
       'limit':'158个本地文件匹配提供方blobID/固定URL；完整tree中间文件截断，不能证明完整源码/传递闭包'},
      {'id':'G0-C07','requirement':'P-02真实上游执行/网络/存储截获与断开拒绝','status':'blocked','evidence':[],
       'limit':'依赖P-01/M-01；独立假循环/本地脚本不满足真实源码替换探针'},
      {'id':'G0-C08','requirement':'J-00双端消费同一kit与失败回执/向量一致','status':'blocked','evidence':[],
       'limit':'等待M-01/P-02实际交接；不创建第二份Schema'},
      {'id':'G0-C09','requirement':'真实平台、账号/模型、隔离、掉电、安装与产品场景','status':'not_run','evidence':[],
       'limit':'84产品场景均未执行；Windows本机清单/静态检查不替代其他平台或真实账号'},
    ]
    for item in checks:
        if item['id'] in {'G0-C05','G0-C06'}:
            item['evidence']=[evidence('runs/plugin-receipt-001/result.json','intermediate_handoff_observation')]
        if item['id']=='G0-C04' and (HERE/'runs/host-kit-002-review/result.json').is_file():
            item['evidence']=[evidence('runs/host-kit-002-review/result.json','limited_kit_checks_candidate_acceptance_paused')]
            item['limit']='正式002清单/19对向量/13测试/default构建已限定复核；authority测试源变化且宿主通知8x4096最大批次遍历预算问题，暂停签收，等待新候选ready'
        if item['id']=='G0-C04' and (HERE/'runs/host-kit-003-review/result.json').is_file():
            item.update(status='verified',evidence=[evidence('runs/host-kit-003-review/result.json','four_family_experimental_rust_kit')],
              limit='003正式静止候选：180文件、14测试含8x4096消费回归、38独立语义解码、40向量文件重产通过；仅四类实验Rust/fake，无完整M01/生产后端/Dart-C资格')
        if item['id'] in {'G0-C05','G0-C06'} and (HERE/'runs/plugin-ready-review/result.json').is_file():
            item['evidence']=[evidence('runs/plugin-ready-review/result.json','P00_entry_P01_partial_static_handoff')]
            item['limit']=('唯一ready交接17冻结输入、43测试+12CLI预期回执一致；native/Wasm产品图仍0/2，完整P00阻塞'
              if item['id']=='G0-C05' else '209+13=222文件集合/双摘要/许可核对；固定来源是partial_snapshot，旧截断tree明确排除，真实源码闭包与构建资格仍阻塞')
    third='runs/p02-native-probe-001-review-004/result.json'
    third_ready=(HERE/third).is_file() and json.loads((HERE/third).read_text(encoding='utf-8')).get('status')=='verified_limited'
    final_identity='runs/p02-native-probe-001-review-004/final-identity.json'
    third_ready=third_ready and (HERE/final_identity).is_file() and json.loads((HERE/final_identity).read_text(encoding='utf-8')).get('status')=='verified'
    if third_ready:
        for item in checks:
            if item['id'] in {'G0-C05','G0-C06','G0-C07','G0-C08'}:
                item['evidence'].append(evidence(third,'complete_sources_and_independent_limited_net_refusal'))
                item['evidence'].append(evidence(final_identity,'consumer_relocation_kit_and_build_identity'))
                item['limit']={
                  'G0-C05':'固定完整源码获取已解决；独立探针离线编译不等于产品构建，native/Wasm产品图仍0/2',
                  'G0-C06':'Codex 8697与CC Switch 1322文件及全部Git树独立重算一致，源码下载阻塞已解除；固定fork补丁与598包探针图复核通过，完整产品闭包/目标feature资格尚缺',
                  'G0-C07':'保持原Rust源码/锁的joint consumer独立离线编译并重放2拒绝场景/15断言通过；仅Responses入口，无Core ModelClient循环、exec/store接管、成功SSE、真实宿主IPC或完整无旁路证明',
                  'G0-C08':'003同源kit进入真实Responses调用拒绝探针；宿主仍为进程内断开fixture，真实P02双端联调未闭合'
                }[item['id']]
    fourth='runs/p02-exec-store-002-review-001/result.json'
    fourth_ready=(HERE/fourth).is_file() and json.loads((HERE/fourth).read_text(encoding='utf-8')).get('status')=='verified_limited'
    if fourth_ready:
        for item in checks:
            if item['id'] in {'G0-C05','G0-C06','G0-C07','G0-C08'}:
                item['evidence'].append(evidence(fourth,'source_and_producer_build_review_independent_exec_store_replay'))
                item['limit']={
                  'G0-C05':'固定源码获取已解决；网络及exec/store资格图不是产品图，native/Wasm产品图仍0/2；本轮复跑生产方原exe，未独立编译Core',
                  'G0-C06':'完整固定Codex树与5文件163行独立工作副本补丁复核；mxc1740/nucleo42文件及Git树通过，两新探针锁身份核对；完整产品闭包/目标feature资格未齐',
                  'G0-C07':'网络限定拒绝批已验；本轮真实LiveThread6案12断言、Core prepared unified-exec3案27断言独立运行通过；仍缺Core ModelClient四路、成功/持久化与执行生命周期、带decider入口及完整无旁路资格',
                  'G0-C08':'003同源kit用于三个有限接缝的本地拒绝探针；实际宿主IPC、M04/M06生产后端及P02完整双端联调未闭合'
                }[item['id']]
    fifth='runs/p02-core-network-003-review-001/result.json'
    fifth_ready=(HERE/fifth).is_file() and json.loads((HERE/fifth).read_text(encoding='utf-8')).get('status')=='verified_limited'
    if fifth_ready:
        for item in checks:
            if item['id'] in {'G0-C05','G0-C06','G0-C07','G0-C08'}:
                item['evidence'].append(evidence(fifth,'Core_selected_network_branches_independent_pinned_exe_replay'))
                item['limit']={
                  'G0-C05':'资格探针的生产编译证据和独立原exe运行已验；native/Wasm产品图仍0/2，未独立编译本批Core',
                  'G0-C06':'本批固定8697/新8699文件及5文件391行补丁复核通过，1113包锁身份核验；各批独立工作副本尚非集成产品闭包',
                  'G0-C07':'LiveThread/exec有限拒绝已验；本批Core HTTP/WS/prewarm/preconnect/合成426自然fallback及同一ModelClient新session保持HTTP，6案47计数断言独立运行通过；成功流/reconnect/auth/execute/Realtime及完整接管未验',
                  'G0-C08':'本地限定资格通过不等于实际宿主IPC；M03/M08网络与认证、M04持久化、M06执行生命周期后端及完整P02双端联调未闭合'
                }[item['id']]
    sixth_audit='runs/p02-integration-004-readonly-001/result.json'
    sixth='runs/p02-integration-004-replay-001/result.json'
    sixth_ready=all((HERE/p).is_file() and json.loads((HERE/p).read_text(encoding='utf-8')).get('status')==s
                    for p,s in [(sixth_audit,'verified_read_only'),(sixth,'verified_limited')])
    if sixth_ready:
        for item in checks:
            if item['id'] in {'G0-C05','G0-C06','G0-C07','G0-C08'}:
                item['evidence'].extend([evidence(sixth_audit,'integrated_source_features_and_producer_build'),
                                         evidence(sixth,'independent_pinned_integrated_exe_replay')])
                item['limit']={
                  'G0-C05':'同一受限资格构建的三接缝24案109计数断言独立复跑通过，未独立编译；native/Wasm产品图仍0/2',
                  'G0-C06':'固定8697/新8700文件、12处693行集成补丁、1117包锁及905编译artifact身份通过；90个Codex路径包同根，仍非产品目标/feature闭包',
                  'G0-C07':'同一exe集成真实LiveThread、prepared exec、Core网络，新增共享生命周期与新Tokio task守卫已独立复现；仅选定入口拒绝，对象释放不等于writer释放，生产授权/成功后端/OS旁路仍缺',
                  'G0-C08':'同构建本地资格已验；真实宿主IPC、M02授权及M03/M08/M04/M06生产生命周期尚未闭合'
                }[item['id']]
    checklist={'format_version':1,'captured_at':datetime.now(timezone.utc).isoformat(),'gate':'G0','status':'blocked',
      'source':{'path':str(PLAN/'03_契约交接与验收.md'),'lines':[69,77,180]},'checks':checks,
      'product_scenarios':{'total':84,'verified':0,'not_run':84},
      'findings':[{'id':'J-F01','severity':'blocker','status':'open','description':'G0要求P-02真实三接缝，不可凭P-00/P-01与最小kit闭门','source':'03_契约交接与验收.md:69,77; 02_插件侧编码构建路径.md:27'},
                  {'id':'J-F02','severity':'boundary','status':'open','description':'最小四类kit不等于完整七组契约后端；SDK仍experimental','source':'01_Morrow侧扩充实施路径.md:55; 03_契约交接与验收.md:180'}]}
    write_json(HERE/'g0-checklist.json',checklist)
    cat=json.loads((HERE/'index.json').read_text(encoding='utf-8'))
    m00=next(i for i in cat['work_packages'] if i['id']=='M-00')
    m00.update(status='verified',evidence=[evidence('runs/m00-handoff-001/result.json','baseline_work_package')],
               status_reason='M-00首轮基线台账/178输入与7计划文件/前置检查复核；不证明新能力实现或旧guest运行资格')
    if (HERE/'runs/host-kit-003-review/result.json').is_file():
        next(i for i in cat['work_packages'] if i['id']=='M-01').update(status='blocked',
          evidence=[evidence('runs/host-kit-003-review/result.json','four_family_experimental_rust_kit')],
          status_reason='003四类实验Rust/fake最小kit已独立验证；完整七类/Dart-C绑定/后端未齐，不能标完整M01结项')
    if (HERE/'runs/plugin-ready-review/result.json').is_file():
        for ident,reason in [('P-00','入口/失败路径回执已核对；完整来源和native/Wasm产品图0/2，完整门槛阻塞'),
                             ('P-01','222固定文件和静态闭包审查已交接；partial snapshot不是可构建的目标/feature最小闭包')]:
            next(i for i in cat['work_packages'] if i['id']==ident).update(status='blocked',
              evidence=[evidence('runs/plugin-ready-review/result.json','P00_entry_P01_partial_static_handoff')],status_reason=reason)
        gate=next(i for i in cat['gates'] if i['id']=='G0')
        gate.update(status='blocked',evidence=[evidence('runs/host-kit-003-review/result.json','four_family_experimental_rust_kit'),
          evidence('runs/plugin-ready-review/result.json','P00_entry_P01_partial_static_handoff')],
          status_reason='最小宿主kit及插件独立切片已核验，缺可构建固定上游/两产品图/P02真实exec-net-store截获及断开拒绝')
        next(i for i in cat['work_packages'] if i['id']=='J-00').update(status='blocked',evidence=gate['evidence'],
          status_reason='索引/独立检查与正式kit消费复核已完成；真实插件P02双端联调未实施，J00整项未闭合')
    if third_ready:
        current=evidence(third,'complete_sources_and_independent_limited_net_refusal')
        extra=evidence(final_identity,'consumer_relocation_kit_and_build_identity')
        for ident,reason in [
          ('P-00','原入口交接已核验且完整固定源码获取阻塞解除；独立探针编译通过，native/Wasm产品图仍0/2'),
          ('P-01','完整固定源码双摘要/Git树、声明fork补丁及598包探针解析图已独立核验；产品目标/feature最小闭包和两产品图仍未齐'),
          ('P-02','真实Responses入口2拒绝场景/15断言独立重放通过；exec/store、Core ModelClient、成功SSE、真实宿主IPC及完整无旁路资格尚缺'),
          ('J-00','已独立消费003 kit并重放真实Responses入口拒绝路径；真实宿主IPC及P02三接缝双端联调未闭合')]:
            item=next(i for i in cat['work_packages'] if i['id']==ident)
            prior=[e for e in item.get('evidence',[]) if e['path'] not in {current['path'],extra['path']}]
            item.update(status='blocked',evidence=prior+[current,extra],status_reason=reason)
        gate=next(i for i in cat['gates'] if i['id']=='G0')
        gate['evidence'].append(current)
        gate['evidence'].append(extra)
        gate['status_reason']='完整固定源码获取阻塞解除；003最小kit与真实Responses入口拒绝探针独立验证通过，仍缺两产品图/P02完整exec-net-store及实际双端联调'
    if fourth_ready:
        current=evidence(fourth,'source_and_producer_build_review_independent_exec_store_replay')
        for ident,reason in [
          ('P-00','入口与固定完整源码已核验；资格探针不等于产品构建，native/Wasm产品图仍0/2'),
          ('P-01','完整源码与新5文件补丁、mxc/nucleo固定树及两个资格探针锁身份已核验；产品目标/feature闭包仍未齐'),
          ('P-02','Responses限定拒绝批及LiveThread6案12/Core prepared unified-exec3案27独立复跑通过；Core ModelClient四路、持久化/执行成功生命周期、带decider入口和完整无旁路资格尚缺'),
          ('J-00','003同源kit三个有限接缝的本地拒绝探针已核验；真实宿主IPC/M04/M06后端与完整P02双端联调未完成')]:
            item=next(i for i in cat['work_packages'] if i['id']==ident)
            prior=[e for e in item.get('evidence',[]) if e['path']!=current['path']]
            item.update(status='blocked',evidence=prior+[current],status_reason=reason)
        gate=next(i for i in cat['gates'] if i['id']=='G0')
        gate['evidence']=[e for e in gate.get('evidence',[]) if e['path']!=current['path']]+[current]
        gate['status_reason']='固定源码及三个有限接缝本地拒绝探针已核验；仍缺Core ModelClient完整路由、M04/M06真实后端及IPC、两产品图与完整P02资格'
    if fifth_ready:
        current=evidence(fifth,'Core_selected_network_branches_independent_pinned_exe_replay')
        for ident,reason in [
          ('P-01','固定源码与各独立资格副本补丁/锁身份已核验；本批网络副本未带batch002执行补丁，尚非集成产品目标/feature闭包'),
          ('P-02','各批有限拒绝已验；本批Core网络6案47独立运行通过，包括自然426回退/跨session状态；成功HTTP/SSE/WS、reconnect/auth、execute/Realtime、生产exec/store及完整无旁路资格尚缺'),
          ('J-00','本地资格回执已核验；各独立副本未构成集成双端产品，真实宿主IPC及M03/M08/M04/M06后端联调未完成')]:
            item=next(i for i in cat['work_packages'] if i['id']==ident)
            prior=[e for e in item.get('evidence',[]) if e['path']!=current['path']]
            item.update(status='blocked',evidence=prior+[current],status_reason=reason)
        gate=next(i for i in cat['gates'] if i['id']=='G0')
        gate['evidence']=[e for e in gate.get('evidence',[]) if e['path']!=current['path']]+[current]
        gate['status_reason']='固定源码及各批有限拒绝/选定Core网络自然回退分支已核验；集成产品图、成功生命周期、真实宿主后端/IPC与完整P02资格仍缺'
    if sixth_ready:
        current=[evidence(sixth_audit,'integrated_source_features_and_producer_build'),
                 evidence(sixth,'independent_pinned_integrated_exe_replay')]
        paths={e['path'] for e in current}
        for ident,reason in [
          ('P-01','固定源码、12处集成补丁、1117包锁及实际编译feature身份已验；三接缝同一受限资格构建成立，产品目标/feature闭包仍未齐'),
          ('P-02','集成原exe独立复跑24案109计数断言通过，含共享对象生命周期与8个新task守卫；仅选定入口拒绝，生产权限/真实后端成功生命周期及完整无旁路资格尚缺'),
          ('J-00','003同源kit和同一集成exe有限资格已独立验证；真实宿主IPC、生产授权/执行/网络/持久化生命周期仍未闭合')]:
            item=next(i for i in cat['work_packages'] if i['id']==ident)
            item.update(status='blocked',evidence=[e for e in item.get('evidence',[]) if e['path'] not in paths]+current,status_reason=reason)
        gate=next(i for i in cat['gates'] if i['id']=='G0')
        gate.update(status='blocked',evidence=[e for e in gate.get('evidence',[]) if e['path'] not in paths]+current,
          status_reason='三接缝同一受限资格exe的24案109断言独立复跑通过；两产品图、真实宿主权限/后端/IPC和完整P02资格仍缺')
    write_json(HERE/'index.json',cat); export_csv(cat)
    print(json.dumps({'g0':'blocked','checklist_checks':len(checks),'M-00':m00['status'],'product_scenarios_verified':0},ensure_ascii=False))

if __name__=='__main__': main()
