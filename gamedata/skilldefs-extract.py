#!/usr/bin/python3

import re
import sys

current_skill = None
current_mode = None
for line in sys.stdin:
    matches = re.search("skilldef ([0-9]*):([0-9]*)", line)
    if matches is not None:
        skill_id = int(matches[1])
        skill_class = int(matches[2])
        current_skill = (skill_id, skill_class)

    matches = re.search("----mode: (.*)", line)
    if matches is not None:
        current_mode = matches[1].strip()

    if current_mode == "0":
        matches = re.search(r"([0-9]+\.[0-9]+)\s+\(Damage Multiplier\)", line)
        if matches is not None:
            multiplier = matches[1]
            if current_skill[1] == 0:
                print(f"{current_skill[0]} {multiplier}")

print("WARNING: Adding manual data from 2021-03-28", file=sys.stderr)
print("Icerazor's Ire 43856: 0.3", file=sys.stderr)
print("43856 0.3")
print("Embrace the Darkness 34331: 0.3", file=sys.stderr)
print("34331 0.3")
print("Echoing Eruption 27964: 1", file=sys.stderr)
print("27964 1.0")
print("Shattershot 40497: 0.65", file=sys.stderr)
print("40497 0.65")
print("Spiritcrush 43993: 0.5", file=sys.stderr)
print("43993 0.5")
print("Citadel Bombardment 42836: 0.452", file=sys.stderr)
print("42836 0.452")


print("WARNING: There's a hack for Searing Fissure (28357) in the code, make sure that data is updated as well.",
      file=sys.stderr)
