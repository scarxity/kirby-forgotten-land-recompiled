# Kirby and the Forgotten Land: the build's game values (the Makefile includes this; game/game.h holds the runtime's).
# The version is here, not in game.h: game.h is part of the game code's id, and a new release of the app alone must
# not make players rebuild their game code.
EXE_NAME     := ForgottenLandRecomp
GAME_VERSION := 0.4.0
