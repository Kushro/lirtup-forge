"""Measure WCAG 2.1 contrast of the color roles in lf.css, for both themes."""
import re,sys
css=open(sys.argv[1]).read()
def block(sel):
    m=re.search(re.escape(sel)+r'\s*\{(.*?)\n\}',css,re.S); return dict(re.findall(r'--([\w-]+):\s*(#[0-9A-Fa-f]{6})',m.group(1)))
def lum(h):
    c=[int(h[i:i+2],16)/255 for i in (1,3,5)]
    c=[x/12.92 if x<=0.03928 else ((x+0.055)/1.055)**2.4 for x in c]
    return 0.2126*c[0]+0.7152*c[1]+0.0722*c[2]
def cr(a,b):
    la,lb=sorted([lum(a),lum(b)],reverse=True); return (la+0.05)/(lb+0.05)
pairs=[(f'on-{r}',r,4.5) for r in ['primary','primary-container','secondary','secondary-container','tertiary','tertiary-container','error','error-container','success-container']]
surf=['surface','surface-container-lowest','surface-container-low','surface-container','surface-container-high','surface-container-highest']
for s in surf:
    pairs+= [('on-surface',s,4.5),('on-surface-variant',s,4.5),('primary',s,4.5),('tertiary',s,4.5),('error',s,4.5),('success',s,4.5),('outline',s,3),('outline-variant',s,1.0)]
pairs+=[('inverse-on-surface','inverse-surface',4.5),('inverse-primary','inverse-surface',4.5),('surface','success',4.5)]
for name,sel in [('oscuro','.lf-dark, .lf {'),('claro','.lf-light {')]:
    t=block(sel[:-2]); bad=0
    for fg,bg,need in pairs:
        if need<=1: continue
        r=cr(t[fg],t[bg])
        if r<need: bad+=1; print(f'{name}: {fg} sobre {bg} = {r:.2f} (< {need})')
    print(name,'fallos:',bad,'de',sum(1 for p in pairs if p[2]>1))
