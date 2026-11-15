#!/usr/bin/env python3
"""Write the synthetic enhance fixtures: gamedata.json + strings.ini (BOM, CRLF).

All names and prose are made up; numbers are chosen to hit the spec's reference values
and format-table edge cases."""
import json, sys, os

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
E = "EntityClassDefinition."
ini = []  # (key, value)

def k(key, value):
    ini.append((key, value))

def item(short, typ, size, grade, name=True, desc=True, sub=None, mfr="ZETA", short_key=False):
    loc = {}
    if name:
        loc["name"] = f"@item_Name{short}"
    if short_key:
        loc["shortName"] = f"@item_Name{short}_short"
    if desc:
        loc["desc"] = f"@item_Desc{short}"
    d = {"recordName": E + short, "id": f"id-{short.lower()}", "loc": loc, "type": typ, "size": size, "grade": grade, "manufacturer": mfr}
    if sub:
        d["subType"] = sub
    return d

comps = []
def comp(short, cat, typ, size, grade, stats, cls=None, extra=None, **kw):
    c = item(short, typ, size, grade)
    c.update({"category": cat, "stats": stats})
    c.update(kw)
    if extra:
        c["detail"] = extra
    comps.append(c)
    k(f"item_Name{short}", f"Filler {short.split('_')[-1]}")
    head = f"Item Type: Filler\\nSize: {size}\\n" + (f"Class: {cls}\\n" if cls else "")
    k(f"item_Desc{short}", head + "\\nLorem filler prose for a made-up part.\\n ")

res = lambda a, b: {"min": a, "max": b}
comp("SHLD_ZETA_S01_Wall", "shields", "Shield", 1, 2,
     {"MaxShieldHealth": 7480, "MaxShieldRegen": 1346, "DownedRegenDelay": 8.5, "DamagedRegenDelay": 4.2},
     cls="Military", power=2, em=500, ir=0, health=300, distortionMax=2450,
     extra={"resistance": {"physical": res(0, .25), "energy": res(0, .1), "distortion": res(0, .9), "thermal": res(0, 0)}})
comp("SHLD_ZETA_S02_Dome", "shields", "Shield", 2, 3,
     {"MaxShieldHealth": 12000.4, "MaxShieldRegen": 0, "DownedRegenDelay": 10, "DamagedRegenDelay": 0},
     cls="Stealth", power=0.4, em=0, ir=0, health=0)
comp("QDRV_ZETA_S01_Hop", "quantumDrives", "QuantumDrive", 1, 1,
     {"driveSpeed": 301_000_000, "spoolUpTime": 8, "cooldownTime": 16.5, "quantumFuelRequirement": 0.0054,
      "stageOneAccelRate": 6936600, "stageTwoAccelRate": 10494000, "calibrationRate": 1000,
      "minCalibrationRequirement": 5000, "maxCalibrationRequirement": 10000},
     cls="Civilian", power=2, em=15000, ir=0, health=170, distortionMax=2600)
comp("RADR_ZETA_S01_Ear", "radars", "Radar", 1, 4, {"cooldownTime": 2.5}, cls="Competition",
     power=4, em=1440, health=630, extra={"signatureDetection": [{"sensitivity": 1.1}]})
comp("POWR_ZETA_S01_Glow", "powerPlants", "PowerPlant", 1, 2, {"powerOutput": 20}, cls="Industrial", em=6550, ir=0, health=140)
comp("COOL_ZETA_S01_Chill", "coolers", "Cooler", 1, 1, {"coolantOutput": 46}, power=2, em=818, ir=5890, health=69, distortionMax=1050)
comp("MRCK_ZETA_S03_Quad", "missileRacks", "MissileLauncher", 3, 1, {"missileCount": 4, "missileSize": 3}, health=200)
comp("LFSP_ZETA_S01_Air", "lifeSupport", "LifeSupportGenerator", 1, 1, {}, power=1, health=100)
comp("THRS_ZETA_Main", "thrusters", "MainThruster", 1, 1, {}, health=50)  # one common line: no block
# A key shared by two paint variants with equal stats (namesake step 4) and one with a
# different twin (step 5).
for v in ["Red", "Blue"]:
    c = item(f"COOL_ZETA_S02_Frost_{v}", "Cooler", 2, 2)
    c["loc"] = {"name": "@item_NameCOOL_ZETA_S02_Frost", "desc": "@item_DescCOOL_ZETA_S02_Frost"}
    c.update({"category": "coolers", "stats": {"coolantOutput": 60}, "health": 200})
    comps.append(c)
k("item_NameCOOL_ZETA_S02_Frost", "Filler Frost")
k("item_DescCOOL_ZETA_S02_Frost", "Shared frost prose.")
for v, out_ in [("A", 30), ("B", 40)]:
    c = item(f"POWR_ZETA_S02_Twin{v}", "PowerPlant", 2, 1)
    c["loc"] = {"name": "@item_NamePOWR_ZETA_Twin", "desc": "@item_DescPOWR_ZETA_Twin"}
    c.update({"category": "powerPlants", "stats": {"powerOutput": out_}})
    comps.append(c)
k("item_NamePOWR_ZETA_Twin", "Filler Twin")
k("item_DescPOWR_ZETA_Twin", "Twin prose.")
# A template that must be ignored.
t = item("SHLD_Template", "Shield", 1, 1)
t.update({"category": "shields", "isTemplate": True, "stats": {"MaxShieldHealth": 1}})
comps.append(t)

def mode(kind, name, rate=None, alpha=None, dps=None, **kw):
    m = {"name": name, "kind": kind}
    if rate is not None: m["fireRate"] = rate
    if alpha is not None: m["alpha"] = alpha
    if dps is not None: m["dps"] = dps
    m.update(kw)
    return m

weapons = []
def weapon(short, cls, typ, size, modes, ammo=None, sub="Gun", name_value=None, desc=True, **kw):
    w = item(short, typ, size, 1, sub=sub, desc=desc)
    w.update({"class": cls, "modes": modes})
    if ammo: w["ammo"] = ammo
    w.update(kw)
    weapons.append(w)
    k(f"item_Name{short}", name_value or f"Filler {short.split('_')[1]}")
    if desc:
        k(f"item_Desc{short}", "Filler gun prose.")
    return w

# Reference: 102.5 x 900 RPM = 1,537.5 DPS; 1,251 m/s x 3.2 s = 4,003 m.
weapon("ZETA_BallisticGatling_S5", "ship", "WeaponGun", 5,
       [mode("rapid", "Rapid", 900, 102.5, 1537.5, pellets=1)],
       {"recordName": "AmmoParams.z5", "speed": 1251, "lifetime": 3.2, "range": 4003, "damage": {"physical": 102.5}, "capacity": 5004},
       damageType="physical", dps=1537.5, alpha=102.5, fireRate=900, power=0.1, em=36, ir=0, health=1650)
weapon("ZETA_LaserCannon_S2", "ship", "WeaponGun", 2,
       [mode("sequence", "Single", 100, 225.4, 375.67)],
       {"recordName": "AmmoParams.zc", "speed": 1184, "lifetime": 2.5338, "range": 3000, "damage": {"energy": 225.4}},
       damageType="energy", dps=375.67, power=2, em=41, ir=0, health=550,
       capacitor={"shots": 25, "costPerShot": 90, "requestedRegenPerSecond": 1125, "regenCooldown": 1.4, "regenPerSecond": 3})
weapon("ZETA_Scatter_S3", "ship", "WeaponGun", 3,
       [mode("single", "Single", 40, 396, 264, pellets=8, damageMultiplier=1)],
       {"recordName": "AmmoParams.zs", "speed": 900, "lifetime": 1, "range": 900, "damage": {"energy": 49.5}, "capacity": 60},
       damageType="energy", dps=264, power=1, em=450, health=1550)
weapon("ZETA_Mixed_S1", "ship", "WeaponGun", 1,
       [mode("rapid", "Rapid", 600, 150, 1500)],
       {"recordName": "AmmoParams.zm", "speed": 1000, "lifetime": 1.5, "range": 1500, "damage": {"physical": 120, "energy": 30}, "capacity": 500},
       damageType="physical", dps=1500, health=400)
weapon("ZETA_TractorBeam_S1", "ship", "TractorBeam", 1, [], sub=None, power=3, em=200, health=500)
weapon("ZETA_MiningLaser_S1", "ship", "WeaponMining", 1,
       [mode("beam", "Beam", dps=3900, zeroDamageRange=45, beamDamage={"energy": 3900})],
       miningModifiers={"laserInstability": 35, "resistanceModifier": 25, "optimalChargeWindowSizeModifier": 40}, power=1, health=8000)
weapon("ZETA_RocketPod_S1", "ship", "WeaponGun", 1, [mode("rapid", "Rapid", 300, 50, 250)],
       {"recordName": "AmmoParams.zr", "speed": 300, "lifetime": 5, "range": 1500, "damage": {"physical": 50}, "capacity": 12},
       sub="Rocket", damageType="physical", dps=250)
# The spec's Bengal S6 / APAR S4 case, with filler text.
apar = weapon("APAR_BallisticGatling_S4", "ship", "WeaponGun", 4, [mode("rapid", "Rapid", 1200, 63.3, 1266)],
       {"recordName": "AmmoParams.a4", "speed": 1296, "lifetime": 2.2, "range": 2851, "damage": {"physical": 63.3}, "capacity": 3660},
       damageType="physical", dps=1266)
beng = weapon("Bengal_BallisticGatling_S6", "ship", "WeaponGun", 6, [mode("rapid", "Rapid", 1000, 200, 3333.33)],
       {"recordName": "AmmoParams.b6", "speed": 1300, "lifetime": 3, "range": 3900, "damage": {"physical": 200}, "capacity": 9000},
       damageType="physical", dps=3333.33, desc=False)
ini.pop()  # Bengal has no keys of its own
beng["loc"] = dict(apar["loc"])
# FPS
weapon("zeta_pistol_01", "fps", "WeaponPersonal", 1,
       [mode("single", "Single", 180, 60, 180)],
       {"recordName": "AmmoParams.zp", "magazine": "zeta_pistol_01_mag", "speed": 550, "lifetime": 2, "range": 1100,
        "damage": {"physical": 60}, "capacity": 6, "falloff": {"physical": {"minDistance": 55, "perMeter": 0.2, "floor": 35}}},
       sub="Small", damageType="physical", dps=180, mass=1.3)
weapon("zeta_rifle_01", "fps", "WeaponPersonal", 3,
       [mode("single", "Single", 300, 20, 100), mode("rapid", "Rapid", 900, 20, 300), mode("burst", "Burst", 900, 20, 133.33, shotCount=3, sustainedFireRate=400)],
       {"recordName": "AmmoParams.zr2", "speed": 900, "lifetime": 1.5, "range": 1350, "damage": {"physical": 20}, "capacity": 30},
       sub="Medium", damageType="physical", dps=300, mass=4.25)
weapon("zeta_smg_energy_01", "fps", "WeaponPersonal", 2,
       [mode("beam", "Beam", dps=275, zeroDamageRange=25, beamDamage={"energy": 275})],
       {"recordName": "AmmoParams.ze", "capacity": 45}, sub="Small", mass=1.6)
weapon("zeta_shotgun_01", "fps", "WeaponPersonal", 3,
       [mode("single", "Single", 60, 80, 80, pellets=8)],
       {"recordName": "AmmoParams.zg", "speed": 300, "lifetime": 0.5, "range": 150, "damage": {"physical": 10}, "capacity": 8,
        "falloff": {"physical": {"minDistance": 5, "perMeter": 0, "floor": 1}}},
       sub="Medium", damageType="physical", dps=80, mass=4)
weapon("zeta_lmg_01", "fps", "WeaponPersonal", 4,
       [mode("rapid", "Rapid", 650, 14.5, 157.08)],
       {"recordName": "AmmoParams.zl", "speed": 600, "lifetime": 2, "range": 1200, "damage": {"physical": 14.5}, "capacity": 75,
        "falloff": {"physical": {"minDistance": 50, "perMeter": 0.0137, "floor": 10}}},
       sub="Medium", damageType="physical", dps=157.08, mass=6.75)
weapon("zeta_medgun_01", "fps", "WeaponPersonal", 1, [], sub="Gadget")  # tool: no block

missiles = []
def missile(short, size, dmg, **kw):
    m = item(short, "Missile", size, 1)
    m.update({"damage": dmg})
    m.update(kw)
    missiles.append(m)
    k(f"item_Name{short}", f"Filler {short.split('_')[-1]}")
    k(f"item_Desc{short}", "Filler missile prose.")

missile("MISL_S02_IR_ZETA_Spark", 2, {"physical": 1700}, explosionRadius=3, trackingSignal="Infrared", lockTime=1.5,
        lockRangeMin=1800, lockRangeMax=10000, speed=1250, maxLifetime=30, health=25)
missile("MISL_S03_EM_ZETA_Bolt", 3, {"physical": 12500, "energy": 14500}, explosionRadius=10, trackingSignal="Electromagnetic",
        lockTime=2, lockRangeMin=50, lockRangeMax=15000, speed=900, maxLifetime=40)
missile("MISL_S01_CS_ZETA_Dart", 1, {"physical": 400}, trackingSignal="CrossSection", lockTime=0.4, lockRangeMax=6000, speed=1400)
missile("BOMB_S03_ZETA_Drop", 3, {"physical": 27000}, isBomb=True, explosionRadius=40, speed=0, maxLifetime=60, health=100)
missile("MISL_S04_IR_ZETA_Lance", 4, {"distortion": 6000}, trackingSignal="Infrared", lockTime=3, lockRangeMin=2000, lockRangeMax=25000, speed=686, maxLifetime=45)
# Name key missing from stock: no entry at all.
m = item("MISL_S01_IR_ZETA_Ghost", "Missile", 1, 1)
m.update({"damage": {"physical": 1}, "trackingSignal": "Infrared"})
m["loc"] = {"name": "@item_NameMISSING_Ghost"}
missiles.append(m)

ships = []
def ship(short, crew, bbox, flight, loadout, armor, totals, name_key=None, desc_key=None, career="@vehicle_focus_combat", role="@vehicle_class_fighter"):
    s = {"recordName": E + short, "id": f"id-{short.lower()}",
         "loc": {"name": name_key or f"@vehicle_Name{short}", "desc": desc_key or f"@vehicle_Desc{short}", "career": career, "role": role},
         "crewSize": crew, "boundingBox": bbox, "flight": flight, "loadout": loadout, "armor": armor, "totals": totals}
    ships.append(s)
    if not name_key:
        k(f"vehicle_Name{short}", f"Filler {short.split('_')[1]}")
    if not desc_key:
        k(f"vehicle_Desc{short}", "Manufacturer: Filler\\nFocus: Filler\\n\\nShip filler prose.")

li = lambda item, size, count=1, **kw: dict({"item": item, "size": size, "count": count}, **kw)
fl = lambda scm, mx, bf, bb, p, y, r: {"controller": "Controller_Flight_X", "scmSpeed": scm, "maxSpeed": mx,
                                       "boostSpeedForward": bf, "boostSpeedBackward": bb, "pitch": p, "yaw": y, "roll": r}
arm = lambda h: {"item": "ARMR_X", "health": h, "damageMultiplier": {"physical": 0.75, "energy": 0.6, "distortion": 1, "thermal": 1, "stun": 1}}
ship("ZETA_Gladiator", 1, {"x": 16.5, "y": 21, "z": 5.5}, fl(226, 1193, 520, 268, 71, 63, 195),
     {"weapons": [li("ZETA_Mixed_S1", 3, 2, type="WeaponGun"), li("ZETA_LaserCannon_S2", 2, 1, type="WeaponGun")],
      "missiles": [li("MISL_S03_EM_ZETA_Bolt", 3, 4)], "shields": [li("SHLD_ZETA_S01_Wall", 1, 2)],
      "powerPlants": [li("POWR_ZETA_S01_Glow", 1)], "coolers": [li("COOL_ZETA_S01_Chill", 1, 2)], "quantumDrives": [li("QDRV_ZETA_S01_Hop", 1)]},
     arm(3300), {"cargoScu": 12, "fuel": 28, "quantumFuel": 1.2})
# Spec reference: Prowler flight controller pitch 38, roll 110, yaw 32.
ship("ZETA_Prowl", 2, {"x": 32, "y": 34, "z": 15}, fl(212, 1200, 450, 240, 38, 32, 110),
     {"weapons": [li("ZETA_BallisticGatling_S5", 5, 2, type="WeaponGun"), li("ZETA_TractorBeam_S1", 1, 1, type="TractorBeam")]},
     arm(12150), {"fuel": 7.8})
ship("ZETA_Hauler", 4, {"x": 74, "y": 126, "z": 30}, fl(140, 1050, 315, 160, 16, 16, 30),
     {"weapons": [li("ZETA_BallisticGatling_S5", 4, 8, type="WeaponGun")], "shields": [li("SHLD_ZETA_S02_Dome", 3, 2)]},
     None, {"cargoScu": 456, "fuel": 40.6, "quantumFuel": 20.9}, career="@vehicle_focus_transport", role="@vehicle_class_freight")
ship("ZETA_Buggy", 1, {"x": 1.6, "y": 5.2, "z": 1.9}, {"controller": "C", "scmSpeed": 25}, {"shields": [li("SHLD_X", 0)]}, None, None,
     role="@LOC_EMPTY")
ship("ZETA_Bare", None, None, None, None, None, None, career=None, role=None)  # nothing to show: no block
# Player hull and its AI copy share one key; the AI copy differs and is skipped.
ship("ZETA_Scout", 1, {"x": 10, "y": 12, "z": 3}, fl(250, 1300, 600, 300, 80, 70, 200), None, arm(1000), None)
ship("ZETA_Scout_PU_AI_CRIM", 1, {"x": 10, "y": 12, "z": 3}, fl(240, 1300, 600, 300, 80, 70, 200), None, arm(1000), None,
     name_key="@vehicle_NameZETA_Scout", desc_key="@vehicle_DescZETA_Scout")
k("vehicle_focus_combat", "Combat")
k("vehicle_focus_transport", "Transport")
k("vehicle_class_fighter", "Light Fighter")
k("vehicle_class_freight", "Freight")

def bp(short, produces):
    return {"recordName": "CraftingBlueprintRecord." + short, "id": f"bp-{short.lower()}", "produces": produces,
            "producesNameKey": f"@item_Name{produces}", "tiers": [{"craftTimeSeconds": 60}]}
blueprints = [bp("BP_CRAFT_zeta_pistol_01", "zeta_pistol_01"), bp("BP_CRAFT_zeta_rifle_01", "zeta_rifle_01"),
              bp("BP_CRAFT_SHLD_ZETA_S01_Wall", "SHLD_ZETA_S01_Wall"), bp("BP_CRAFT_ZETA_Scatter_S3", "ZETA_Scatter_S3"),
              bp("BP_CRAFT_zeta_lmg_01", "zeta_lmg_01")]
pools = [{"name": "BP_POOL_A", "blueprints": [{"blueprint": "BP_CRAFT_zeta_rifle_01"}, {"blueprint": "BP_CRAFT_zeta_pistol_01"}, {"blueprint": "BP_CRAFT_SHLD_ZETA_S01_Wall"}]},
         {"name": "BP_POOL_B", "blueprints": [{"blueprint": "BP_CRAFT_ZETA_Scatter_S3"}, {"blueprint": "BP_CRAFT_zeta_lmg_01"}]},
         {"name": "BP_POOL_EMPTY", "blueprints": []}]
standings = [{"name": "Standing_Jr", "recordName": "SReputationStandingParams.Standing_Jr", "displayNameKey": "@rep_standing_jr", "minReputation": 1000},
             {"name": "Standing_Sr", "recordName": "SReputationStandingParams.Standing_Sr", "displayNameKey": "@rep_standing_sr", "minReputation": 9000}]
k("rep_standing_jr", "Filler Junior")
k("rep_standing_sr", "Filler Senior")
k("item_NameZETA_Scrip", "Filler Scrip")

rep = lambda amt, fail=False: {"faction": "f", "amount": amt, "onSuccess": not fail, "onFailure": fail}
contracts = []
def contract(dbg, title, desc, **kw):
    c = {"generator": "Gen_Zeta", "debugName": dbg, "id": f"c-{dbg.lower()}", "titleKey": "@" + title, "descriptionKey": "@" + desc}
    c.update(kw)
    contracts.append(c)
# Guaranteed pool, one rep value, standing, chain, once only.
contract("Zeta_Haul_E", "zeta_haul_title", "zeta_haul_desc", blueprintRewards=[{"pool": "BP_POOL_A", "chance": 1}],
         reputation=[rep(150), rep(-1000, True)], minStanding="Standing_Jr", chain=True, onceOnly=True,
         itemRewards=[{"item": "ZETA_Scrip", "nameKey": "@item_NameZETA_Scrip", "amount": 3}])
# Difficulty variants sharing a title: different pools and rep -> [BP?] [100–300 Rep], Variant sub-headers.
contract("Zeta_Bounty_Easy", "zeta_bounty_title", "zeta_bounty_desc", blueprintRewards=[{"pool": "BP_POOL_A", "chance": 0.25}], reputation=[rep(100)], difficulty="easy")
contract("Zeta_Bounty_Hard", "zeta_bounty_title", "zeta_bounty_desc", blueprintRewards=[{"pool": "BP_POOL_B", "chance": 0.5}], reputation=[rep(300)], difficulty="hard", minStanding="Standing_Sr")
contract("Zeta_Bounty_Free", "zeta_bounty_title", "zeta_bounty_desc", reputation=[rep(200)], difficulty="hard")
# Rep only.
contract("Zeta_Patrol", "zeta_patrol_title", "zeta_patrol_desc", reputation=[rep(8000)])
# Format key title is skipped; description gets a body.
contract("Zeta_Format", "zeta_format_title", "zeta_format_desc", reputation=[rep(50)])
# A description key that is also used as a title gets no body.
contract("Zeta_Odd", "zeta_odd_title", "zeta_patrol_title", reputation=[rep(10)])
# Stock description ends inside an open EM tag (open tail) and one with CIG's unbalanced close.
contract("Zeta_Open", "zeta_open_title", "zeta_open_desc", reputation=[rep(75)])
contract("Zeta_Stray", "zeta_stray_title", "zeta_stray_desc", reputation=[rep(75)])
contract("Zeta_WIP", "zeta_wip_title", "zeta_wip_desc", reputation=[rep(1)], workInProgress=True)
contract("Zeta_Missing", "zeta_missing_title", "LOC_EMPTY", reputation=[rep(5)])
k("zeta_haul_title", "Filler Haul\\n ")
k("zeta_haul_desc", "Filler haul prose at ~mission(Location|Address).\\n")
k("zeta_bounty_title", "Filler Bounty")
k("zeta_bounty_desc", "Filler bounty prose for <EM4>~mission(Contractor)</EM4>.")
k("zeta_patrol_title", "Filler Patrol")
k("zeta_patrol_desc", "Filler patrol prose.")
k("zeta_format_title", "Filler %s Format")
k("zeta_format_desc", "Filler format prose.")
k("zeta_odd_title", "Filler Odd")
k("zeta_open_title", "Filler Open")
k("zeta_open_desc", "Filler prose with an <EM4>open tail")
k("zeta_stray_title", "Filler Stray")
k("zeta_stray_desc", "Filler prose with a stray </EM4> close")
k("zeta_wip_title", "Filler WIP")
k("zeta_wip_desc", "Filler WIP prose.")
k("zeta_missing_title", "Filler Missing")
k("Frontend_PU_Version", "Filler version")
k("ui_unrelated,P", "Unrelated filler")
k("mG_ContractsMgr_Select,P", "Filler select prompt.")

gd = {"schema": 2, "build": None, "components": comps, "weapons": weapons, "missiles": missiles, "ships": ships,
      "contracts": contracts, "blueprints": blueprints, "blueprintPools": pools, "reputationStandings": standings}
json.dump(gd, open(os.path.join(out, "gamedata.json"), "w"), indent=1, sort_keys=True)
body = "; synthetic stock strings for enhance golden tests (made-up filler, no CIG text)\r\n"
body += "".join(f"{a}={b}\r\n" for a, b in ini)
open(os.path.join(out, "strings.ini"), "wb").write(b"\xef\xbb\xbf" + body.encode())
print(len(ini), "keys")
