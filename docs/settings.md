# Settings
The setting module provides the framework governing the group and contact structure of individuals in the model. Philsophically, a setting is a place where transmission can occur.

## Setting Definition
A setting is defined by `SettingId` and a set of `SettingProperties`. A `SettingId` contains the setting category (e.g., home, school, workplace, etc.) and a unique identifier within the given category. Each setting category is associated with `SettingProperties` which contain a parameter for density dependent transmission `alpha`, and `itinerary_specification` which defines the proportion of time an individual interacts in the setting category. This value is also referred to as a ratio. Setting properties are set for each setting category in [model input](model-input.md). We assume that setting properties are uniform across all settings of a certain type. Settings are implemented with the `AnySettingId` trait, which is referenced throughtout the implementation when working with generic setting objects.

## Itineraries and Itinerary Modifiers
Itineraries are a vector of `ItineraryEntry` which store a setting an individual is a member of and a ratio of time spent in the setting. We assume by default that ratio values for itinerary values are those given in `SettingProperties` input for the corresponding setting category. Itineraries are stored in the `SettingsDataContainer`as map between the `PersonId` and itinerary. Upon model initialization, an individuals default itinerary is generated from the synthetic population loader module, where rows of the synthetic population correspond to the setting IDs for a specific person (see [initialization documentation](initialization.md) for more details). The codebase is designed with a specific set of settings in mind. We implement four `CoreSettingTypes`: Home, School, Workplace, and CensusTract. There is a required correspondence between the setting categories listed in `SettingProperties` input and the structure of the synthetic population file. An example of an individual's itinerary is {Home – ID: 1, ratio: 0.33; School – ID: 1, ratio: 0.33; CensusTract – ID: 1, ratio: 0.33}


We assume that an individuals itinerary can modified over the time horizon of the simulation. Three mechanisms listed below define how an itinerary can be modified:
- `ReplaceWith` replace itinerary with a new vector of itinerary entries
- `RestrictTo` reduce the default itinerary to a setting type (e.g., Home)
- `Exclude` exclude a setting type from default itinerary (e.g., Workplace)

The API enables the model developer to call these itinerary modifier methods from other modules (e.g., in a seperate event subscription) to modify the individuals itinerary according to the intended use case. When the itinerary modifier is called, the corresponding new itinerary becomes active and governs the individual's behavior. Lists of active and inactive setting members are stored in the `SettingsDataContainer`. An individual can be an inactive member in a setting if the setting is in one of their itinerary types but not the other type. Modified itineraries are also stored in the `SettingsDataContainer` using a similar map data structure. We assume that an individual can have a single modified itinerary at a time. The itinerary modifier can similarly be removed from an individual (and the map data structure). Without a modified itinerary, the individual will return to following their default itinerary.

Our primary use case for changing itineraries is modeling isolation. We model isolation using the `RestrictTo` mechanism and restricting an individual's itinerary to their home setting.

### Transmission
Settings are used to facilitate transmission. During the infection propogation loop (described in [transmission documentation](transmission.md)), a setting is sampled from the infectious individual's current itinerary, with probability proportional to the normalized ratios across the infector's itinerary. Once a setting is sampled the active members in the setting are equally likely to be sampled to be the infectee of the infection attempt.

Setting properties also impact underlying infection attempt process. As mentioned above, each setting category has a density dependent transmission parameter $\alpha$. These $\alpha$ values are parameters in the individual level infectiousness multipliers that take the form $(N-1)^\alpha$ where $N$ is the number of people in the setting and $\alpha \in [0,1]$. How these multipliers are used to implement rejection sampling is discussed further in the [transmission module documentation](transmission.md).

### Limitations
The settings implemenation is limited in a number of important ways. Firstly, multiple itinerary modifiers cannot be active at the same time for a single individual. This limits the ability to model easily model multiple itinerary modifiers simulataneously. Secondly, itinerary modifiers are not directly linked to changes in person properties like transmission modifiers. This means that any changes in person properties that are meant to also impact itinerary modifiers has to be hard-coded
