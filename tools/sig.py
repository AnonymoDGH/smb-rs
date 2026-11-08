prg=open("analysis/prg.bin","rb").read(); B=0x8000
sigs={
 "PlayerPhysicsSub a5 1d c9 03":"a51dc903",
 "MovePlayerHorizontally 0a0a0a0a8501":"0a0a0a0a8501",
 "IntervalTimer reset a9148d7f07":"a9148d7f07",
 "ScreenTimer=7 a9078da007":"a9078da007",
 "AutoControl 8dfc06a50e":"8dfc06a50e",
 "PlayerEndLevel a90120e6b0":"a90120e6b0",
 "FlagpoleRoutine a2058608b516":"a2058608b516",
 "HandleAxe a9008d7207":"a9008d7207",
 "ParseRow0e lsr5 c95f07":"4a4a4a4a4acd5f07",
 "XPhysics a0008400a51d":"a0008400a51d",
}
for name,h in sigs.items():
    pat=bytes.fromhex(h)
    idx=prg.find(pat)
    print("%-40s %s"%(name, "not found" if idx<0 else "%04X"%(B+idx)))
