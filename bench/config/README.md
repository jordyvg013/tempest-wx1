# Benchmarkconfiguratie

`acceptance.yaml` bevat de drempels voor de eerste formele cross-formatmeting (ADR 0021). Deze waarden zijn vóór die formele meting vastgelegd en mogen na inzage van resultaten niet worden aangepast. Een latere wijziging vereist een nieuwe configversie en expliciete scope-/redenregistratie; historische runs houden de config waarmee ze zijn uitgevoerd.

De drempels zijn doelen, geen behaalde resultaten. De bestaande interne Rust-microbenchmark (ADR 0019) valt buiten deze cross-formatconfiguratie.

Buffered en durable writes zijn aparte modi. Elke adapter moet de daadwerkelijke synchronisatiegarantie documenteren. Een gewone `flush()` geldt niet automatisch als duurzame commit. Als twee formaten geen vergelijkbare durabilitygarantie leveren, zijn die resultaten voor die vergelijking niet equivalent.
