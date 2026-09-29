"""Prepare a new review overlay; never edits the prior product catalogue."""
import hashlib,json
from pathlib import Path
HERE=Path(__file__).resolve().parent
JOINT=HERE.parent
HOST=JOINT.parents[2]
OLD=JOINT/'m02-native-session-001'
def sha(p):return hashlib.file_digest(Path(p).open('rb'),'sha256').hexdigest()
def read(p):return json.loads(Path(p).read_text(encoding='utf-8-sig'))
def write(p,v):
    if p.exists():raise ValueError('refuse overwrite: '+str(p))
    p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def main():
    expected='263be0729c12ff25ac68c5b37b864d0b4ec44eacf7ee66a80cc6f1a4df2e8418'
    if sha(OLD/'ready-handoff.json')!=expected:raise ValueError('previous handoff changed')
    pins={}
    def add(p,s):
        key=str(Path(p).resolve())
        if key in pins and pins[key]!=s:raise ValueError('conflicting frozen identity')
        if sha(key)!=s:raise ValueError('frozen input drift: '+key)
        pins[key]=s
    for x in read(OLD/'runs/independent-sessions-001/identities-before.json'):add(x['path'],x['expected'])
    for x in read(OLD/'runs/final-seal-001/unified-inputs-current.json'):add(x['path'],x['expected'])
    for x in read(OLD/'evidence-manifest.json')['files']:add(x['path'],x['sha256'])
    for name in ['ready-handoff.json','evidence-manifest.json']:add(OLD/name,sha(OLD/name))
    catalogue=read(JOINT/'index.json')
    cases={x['id']:x for x in catalogue['acceptance']}
    defs=[
      ('M02A-01','批准权威及真实复用',['C05','B06'],'追踪创建批准的可信调用至既有审批/生命周期/存储组件；查询/claim不得创建批准','private bool或测试独有内存policy不接受；真实用户审批UI/完整CLI认证未接入须明确'),
      ('M02A-02','批准时固定启动身份',['NX-01','P06'],'批准绑定artifact/config/schema/角色或plugin/slot/operation/能力/授权代次；批准后变更逐项拒绝且无spawn','实际需要字段由宿主权威接口定义；不由联合另造grant wire'),
      ('M02A-03','一次性claim原子性',['P01','B06'],'同一授权并发和重复claim；记录事务消费点、launch pending、实际spawn数和后续状态','原子记录不能与spawn完全原子，须审查中间crash窗口并保守Unknown'),
      ('M02A-04','两个真实host争同slot',['B06','NX-04'],'两个独立host在同库/profile并发，以同或不同批准争同slot；败者明确拒绝且不启动child','不能只测同进程HashMap；无全机枚举，通过己方PID/句柄和spawn事件计数'),
      ('M02A-05','真实历史原字节重放',['NX-02','P01'],'第一实际会话捕获Hello/Query原字节，与host日志逐字节一致；关闭后新会话原样重放并拒绝','不修改epoch，不把synthetic字段篡改计历史重放；分别记录两次身份和自然绑定差异'),
      ('M02A-06','批准后撤销再claim',['P01'],'实际创建批准、撤销、领取顺序；失效授权不能spawn，查询不恢复授权','若仅本host内撤权则明确；跨host可见性与持久状态另列'),
      ('M02A-07','在运行会话撤权',['P01','NX-04'],'跨可信管理入口撤销已claim授权；记录持久代次与实际拒绝/停止顺序','不重复旧普通撤权案充数；在途部分写入本片若未触达仍not_run'),
      ('M02A-08','首次授权deadline',['P01'],'先批准后延迟claim和重复claim跨首次截止；记录同一批准创建时刻与剩余期限','claim/重连不能续期；host重启的时间可信边界需明确，墙钟回退不恢复许可'),
      ('M02A-09','ClosingUnconfirmed跨host挡接替',['NX-06','B05'],'真实holder保持输出；第一host处于Unconfirmed且owner保留时第二host不得spawn','根进程退出/TTL过去均非释放；只观察己方已知holder PID，不全树枚举'),
      ('M02A-10','host crash而child仍活着',['NX-04','R04','B05'],'先持有host/child OS句柄后只终止己方host；独立证实child存活，第二host拒绝或Unknown且无新spawn','测试清理不代表产品自动恢复；OS锁自然释放不得清除持久不确定状态'),
      ('M02A-11','完整释放后新批准启动',['B06','NX-06'],'实际child退出和双EOF、owner释放后新批准可启动；旧批准仍不可复用','对crash未知不能仅因清理成功/检查PID不存在就自动释放'),
      ('M02A-12','锁范围与恢复核对',['B06','R04'],'明确库/profile/用户范围、持久identity、路径别名及锁原语；审查rename/reparse、锁释放和观察/核对API','同一OS用户合作进程边界不等同恶意同用户防篡改或OS强隔离'),
      ('M02A-13','副作用前后故障窗口',['NX-04','R04','P01'],'只读审claim消费、owner占有、spawn、登记child、Release落盘顺序，执行可控真实crash点','不能以失败退出或数据库记录存在代替child退出；注入点与真实崩溃分开'),
      ('M02A-14','输入封存与来源',['L06','QX-01','QX-02','QX-03'],'冻结旧候选、新source/lock/build/exe对应；ready后先审再独立运行并前后校验','不复跑旧10案/codec全套；构建/诊断通过不给84原产品场景信用'),
    ]
    ids=sorted({i for _,_,items,_,_ in defs for i in items})
    matrix={'kind':'joint_diagnostic_overlay_not_wire_or_approval_schema','batch':HERE.name,'prior_handoff_sha256':expected,'original_acceptance':[cases[i] for i in ids],'diagnostics':[{'id':i,'title':t,'original_ids':a,'required_evidence':e,'boundary':b,'status':'not_run','product_pass_credit':0} for i,t,a,e,b in defs],'required_runtime_cases':['authority_fixed_binding','duplicate_claim','two_hosts_same_slot','historical_bytes_replay','revoke_before_claim','revoke_active_via_record','approve_delay_expiry','unconfirmed_blocks_second_host','host_crash_child_alive','released_fresh_approval'],'product_status':{'M-02':'partial','G0':'blocked','G1':'not_passed','P-02':'blocked','J-00':'blocked','graphs':'0/2','acceptance_not_run':84},'observer_constraints':['only known self-created PIDs and held handles; no machine-wide enumeration','do not run before ready source/build/identity review','capture OS witness before crash; absence of a witness is a limitation','finite cleanup only for this batch self-created processes','separate test cleanup from product recovery','private ephemeral DB/profile only; no personal credentials/MCP/content','Capnp001 remains sole guest wire unless host versioned authority supersedes; no reviewer schema']}
    write(HERE/'baseline.json',{'scope':'prior selected source/kit/receipts/ledgers immutable; no runtime compatibility claim','files':[{'path':p,'sha256':s} for p,s in sorted(pins.items())]})
    write(HERE/'matrix.json',matrix)
    print(json.dumps({'frozen_files':len(pins),'diagnostics':len(defs),'original_anchors':len(ids),'matrix_sha256':sha(HERE/'matrix.json'),'baseline_sha256':sha(HERE/'baseline.json')}))
if __name__=='__main__':main()
