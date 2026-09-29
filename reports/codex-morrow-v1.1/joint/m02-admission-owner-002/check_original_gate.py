"""Reuse frozen catalogue verification, writing only this new joint directory."""
import importlib.util,json
from check_review import HERE,JOINT,read,write,sha
def main():
    p=JOINT/'check_joint.py';spec=importlib.util.spec_from_file_location('frozen_joint_catalogue',p);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
    cat=read(JOINT/'index.json');issues=m.verify_catalogue(cat)
    if (JOINT/'coverage.csv').read_bytes()!=m.csv_bytes(cat):issues.append('CSV differs')
    g0=next(x for x in cat['gates']if x['id']=='G0');code=1 if issues else 2 if g0['status']!='verified'else 0
    result={'scope':'original catalogue and source identity only','counts':{'work_packages':len(cat['work_packages']),'gates':len(cat['gates']),'acceptance':len(cat['acceptance'])},'issues':issues,'g0_status':g0['status'],'acceptance_not_run':sum(x['status']=='not_run'for x in cat['acceptance']),'product_scenarios_verified':sum(x['status']=='verified'for x in cat['acceptance']),'exit_code':code,'frozen_checker_sha256':sha(p),'wrapper_sha256':sha(__file__)}
    run=HERE/'runs/final-gate-001';run.mkdir(parents=True,exist_ok=False);write(run/'result.json',result);print(json.dumps(result,indent=2));return code
if __name__=='__main__':raise SystemExit(main())
