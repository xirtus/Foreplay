class_name SkaterController
extends CharacterBody3D

# Tunable Skate Feel Knobs (auto-promoted to foreplay_ir::FeelSpecKnob)
@export_range(0.0, 30.0, 0.5) var max_speed: float = 14.5
@export_range(1.0, 15.0, 0.2) var ollie_impulse: float = 6.8
@export_range(0.1, 1.0, 0.05) var grind_balance_damping: float = 0.95
@export var skater_name: String = "SanVanSkater"
@export var is_grounded: bool = true

# Gameplay signals
signal trick_landed(trick_name: String, score: int)
signal bailed()

func _ready() -> void:
    pass

func _physics_process(delta: float) -> void:
    # Kinematic character controller update loop
    pass

func _on_trick_landed(trick_name: String, score: int) -> void:
    # Camera shake & score banner event
    pass
