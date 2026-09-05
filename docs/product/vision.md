PRODUCT DEFINITION

1. EVERY FEATURE AND THE EXPERIENCE

We are building a Windows-first, local-first AI desktop application.

The application should eventually provide:

- Local text chat
- Streaming LLM responses
- Voice conversations
- Speech-to-text
- Text-to-speech
- Personas
- Conversation history
- Persistent memory
- Local image generation
- Local model management
- GPU/VRAM resource management
- Model loading/unloading
- Model hot-swapping
- Persistent AI characters
- Character profiles
- Character galleries
- Character discovery
- Tinder-style character discovery/swiping
- Character conversations
- Character-specific memories
- Character-consistent image generation

The important requirement is the EXPERIENCE, not merely the existence of features.

CHAT

The user opens the application and can immediately start a conversation with a local AI model.

The experience should feel like a modern messaging application:

- Messages stream naturally.
- The UI remains responsive while generating.
- The user can stop generation.
- Conversations persist automatically.
- Previous conversations can be reopened.
- The user can switch models where supported.
- Errors should be understandable and recoverable rather than exposing raw process failures.

Chat should be the foundation that other modalities build upon rather than having separate isolated conversation systems.

VOICE

The user can enter a voice conversation and speak naturally.

The intended experience is:

Speak → AI understands → AI responds → AI speaks

The user should not have to manually move text between STT, chat, and TTS.

Voice should feel conversational:

- Low perceived delay.
- Streaming where technically possible.
- Ability to interrupt the AI.
- AI stops speaking when interrupted.
- Conversation remains part of the same conversation history.

PERSONAS

The user can select or create an AI persona.

A persona must meaningfully affect how the AI behaves rather than merely existing as profile information.

It should affect things such as:

- Personality
- Tone
- Communication style
- Preferences
- Behavioral tendencies

The application should make it possible to verify that persona information actually reaches the model.

MEMORY

The AI can remember relevant information across conversations.

Memory should feel natural rather than requiring the user to repeatedly remind the AI of important information.

The user should eventually be able to:

- View memories.
- Understand what is remembered.
- Remove memories.
- Correct memories where appropriate.

Memory should prioritize useful information rather than storing every sentence indefinitely.

IMAGE GENERATION

The user can generate images locally.

The intended experience is:

Request → queued/processing state → generation → image appears in conversation/gallery

The UI should communicate that generation is happening and handle cancellation and failures gracefully.

Image generation should participate in application resource management rather than blindly competing with the LLM for GPU memory.

MODEL MANAGEMENT

The user should eventually be able to manage local models from inside the application.

The experience should include:

- Seeing installed models.
- Knowing what capability a model provides.
- Selecting a model.
- Loading/unloading where supported.
- Seeing model status.
- Understanding when a model cannot be loaded because of resource or configuration limitations.

The user should not need to manually launch backend processes.

GPU / VRAM MANAGEMENT

The application should intelligently coordinate AI workloads.

The user should not need to understand CUDA processes, VRAM allocation, or which backend process needs to be killed.

If an image-generation request requires resources currently occupied by the LLM, the application should make an intelligent decision rather than simply crashing with CUDA OOM.

MODEL HOT-SWAPPING

Switching between workloads should eventually feel like an application feature rather than a technical operation.

For example:

"I am chatting with my character and she sends me a picture."

The user should not need to know that the LLM may need to be unloaded, an image model loaded, the image generated, and the LLM restored.

That should happen behind the scenes.

CHARACTERS

Characters are persistent AI entities rather than temporary personas.

A character has:

- Identity
- Appearance
- Personality
- Interests
- History
- Memory
- Relationship state
- Reference images
- Generated images

Opening a character later should feel like returning to the SAME character.

CHARACTER DISCOVERY

The user can browse characters through a swipe/discovery interface.

The intended experience is:

Discover → see character → inspect profile/images → swipe/choose → start conversation

The system should eventually generate many characters without requiring the user to manually construct every profile.

CHARACTER GALLERIES

Each persistent character has a gallery of images associated with them.

The gallery should not simply be a folder of unrelated generations.

Images should be associated with the specific character and contribute to that character's visual history where appropriate.

CHARACTER-SENT IMAGES

A character should eventually be able to send an image during conversation.

For example, a conversation could naturally reach a point where the character decides to send a picture.

The important experience is that the image is generated as something that character would plausibly send, rather than forcing the user into a generic image-generation interface.

PERSISTENT VISUAL IDENTITY

This is a major requirement.

If the user establishes a character with a particular face, hair, body, style, etc., subsequent images should depict the SAME character.

The goal is not necessarily mathematically pixel-perfect identity.

The character should instead be visually consistent enough that a user can look at multiple images and reasonably conclude:

"That is the same character."

The system should use references, conditioning and verification rather than assuming that identical prompts guarantee identical people.


2. CHARACTER SYSTEM IN DEPTH

CHARACTER IDENTITY

Every character needs a stable identity.

That identity must survive:

- Reopening the application.
- New conversations.
- Different conversations.
- New generated images.
- Changes in relationship state.
- Model changes.

A character must therefore be treated as a persistent entity with a stable identifier rather than simply being represented as a prompt.

APPEARANCE

Appearance should be structured rather than existing only inside prose.

Potential characteristics include:

- Face
- Hair
- Eyes
- Skin characteristics
- Body characteristics
- Height/build
- Distinctive features
- Clothing/style
- Accessories
- Other visually important traits

The exact schema should be determined during architecture/data-model research.

The product requirement is that appearance can be used consistently across image generations.

PERSONALITY

Personality should influence actual conversation behavior.

It should affect:

- Vocabulary
- Tone
- Humor
- Emotional responses
- Interests
- Conversational habits
- Preferences
- Reactions to the user
- Communication style

The character should not feel like the same generic LLM wearing a different profile.

MEMORY

Character memory should be conceptually associated with the relationship between the user and that character.

For example:

"The character remembers that I told her about my favorite game."

That memory should remain available later.

Memory should help maintain continuity without requiring the entire historical conversation to be injected into every request.

RELATIONSHIP PROGRESSION

Characters should eventually have persistent relationship progression.

The relationship should develop based on interactions.

Possible progression could conceptually look like:

Stranger
↓
Acquaintance
↓
Friend
↓
Close Friend
↓
Romantic Interest
↓
Partner

These are examples, not a finalized schema.

The important requirement is that relationship state is persistent and can influence future conversations.

The character should remember how the relationship developed.

Relationship progression should not simply be a visible numerical score with no behavioral consequences.

GALLERIES

Each character has a persistent gallery.

The gallery may contain:

- Profile images
- Portraits
- Photos
- Scene images
- Character-generated/sent images
- User-requested images

Images remain associated with the character.

The gallery should also potentially provide reference material for future character image generation.

DISCOVERY / SWIPE FLOW

The intended experience is:

Open Discover
↓
Character appears
↓
View image/profile
↓
Swipe left/right
↓
Next character OR select character
↓
Selected character becomes persistent
↓
Start conversation

A selected character should be saved to the user's collection/history.

The character should not disappear or regenerate into a completely different entity the next time the application opens.

CHARACTER-SENT IMAGES

During conversation, a character may request that the application generate an image representing something the character is saying or doing.

Conceptually:

Conversation
↓
Character decides an image is appropriate
↓
Structured image action
↓
Application validates request
↓
Character identity/reference is loaded
↓
Image generated
↓
Identity consistency checked
↓
If acceptable:
    deliver image
    save to character gallery
↓
If unacceptable:
    regenerate or fail gracefully

The character should not directly produce arbitrary image-generation commands that bypass application validation.

IDENTITY CONSISTENCY TARGET

The desired level is HIGH VISUAL CONTINUITY.

The character should maintain recognizable:

- Face
- Hair
- General physical appearance
- Distinctive features
- Overall visual identity

Across:

- Different poses
- Different clothing
- Different environments
- Different lighting
- Different scenes

Perfect consistency is NOT assumed to be technically achievable.

The actual product requirement is:

"Generated images should be recognizably the same persistent character, with the system actively attempting to preserve identity and rejecting/regenerating obviously inconsistent results."


3. CONCRETE USER FLOWS

FIRST LAUNCH

Open application
↓
Application starts local services
↓
Application checks available models/resources
↓
User sees main interface
↓
User selects or installs an available local model if necessary
↓
User starts first conversation

The first-run experience should be understandable without requiring the user to understand AI infrastructure.

NORMAL CHAT

Open app
↓
Select Chat
↓
Select available AI/persona
↓
Type message
↓
Press Send
↓
Response begins streaming
↓
User can stop generation
↓
Response completes
↓
Conversation is automatically persisted

RESUME CONVERSATION

Open app
↓
Open conversation history
↓
Select conversation
↓
Previous messages appear
↓
Continue conversation

VOICE CONVERSATION

Open app
↓
Open Voice
↓
Microphone activates
↓
User speaks
↓
Speech is transcribed
↓
Message enters conversation
↓
LLM generates response
↓
TTS begins speaking
↓
User can interrupt
↓
Conversation continues

DISCOVER CHARACTER (Tinder look-alike)

Open app
↓
Discover
↓
Character card appears
↓
View profile/images
↓
Swipe
↓
Next character

SELECT CHARACTER

Discover
↓
Find character
↓
Select
↓
Character saved
↓
Character profile opens
↓
Start conversation

CHARACTER CONVERSATION

Open character
↓
Open conversation
↓
Character context is loaded
↓
Relevant character memories are retrieved
↓
Conversation history is loaded
↓
User sends message
↓
Character responds
↓
Important information may become memory
↓
Relationship state can evolve

CHARACTER SENDS IMAGE

Character conversation
↓
Character decides an image is appropriate
↓
Application creates structured image request
↓
Request is validated
↓
Character identity/reference is loaded
↓
Image model receives request
↓
Image generated
↓
Identity consistency checked
↓
If acceptable:
    deliver image
    save to character gallery
↓
If unacceptable:
    regenerate or fail gracefully

IMAGE GENERATION WHILE CHATTING

LLM is active
↓
Image request occurs
↓
Resource manager evaluates GPU state
↓
LLM remains active if resources permit
OR
LLM is temporarily unloaded/suspended
↓
Image generation runs
↓
Image resources released
↓
LLM restored if required
↓
Conversation continues

The user should experience this as one continuous application flow.


4. OFFLINE SCOPE

Once required models/assets have been acquired, the following MUST work without network access:

- Application launch
- UI
- Chat
- LLM inference
- Conversation history
- Personas
- Character profiles
- Character conversations
- Character memory
- Relationship state
- Local image generation
- Character galleries
- Character image generation
- STT
- TTS
- Model management
- GPU/resource management
- Model switching/hot-swapping
- Local database operations
- Local file management
- Existing user settings
- Existing conversations
- Existing characters

The application should not become a nonfunctional shell when the internet disappears.

NETWORK MAY BE USED FOR OPTIONAL OPERATIONS

Potentially:

- Downloading models
- Downloading application updates
- Obtaining optional external resources

These operations must not become dependencies of normal local operation.

The distinction is:

"Downloading something from the internet"

versus

"Requiring the internet to operate."

The first may be allowed.

The second is not acceptable for core functionality.


5. PERFORMANCE EXPECTATIONS

These are PRODUCT EXPERIENCE TARGETS, not yet measured engineering guarantees.

They should eventually be validated against actual hardware/model combinations.

CHAT

The interface should feel responsive immediately.

Desired experience:

- User message appears essentially immediately.
- Generation should ideally begin within roughly 1 to 2 seconds once the model is ready.
- Streaming should begin as soon as the backend produces usable output.
- The UI must never appear frozen while generation occurs.

Exact tokens-per-second targets depend on model size, quantization, context, hardware and runtime configuration.

The important requirement is LOW PERCEIVED LATENCY rather than an arbitrary universal TPS number.

VOICE

Voice should feel conversational.

Desired experience:

- STT begins promptly after the user stops speaking.
- The user should ideally hear the beginning of the response within roughly 1 to 3 seconds depending on STT/LLM/TTS and hardware.
- TTS should stream where possible.
- Interruptions should happen quickly.
- The UI should clearly show listening/thinking/speaking states.

End-to-end voice latency should eventually be measured as:

Speech ends
→ STT
→ LLM first token
→ TTS
→ Audible output

IMAGE GENERATION

Image generation can reasonably take substantially longer than chat.

The user should receive immediate feedback that the job has started.

Desired experience:

Request
↓
Immediate queued/generating state
↓
Progress/status feedback where available
↓
Image appears automatically when complete

The UI should not feel broken simply because generation takes 10, 20, 30+ seconds.

Exact generation time depends on the selected image model and hardware.

MODEL SWITCHING

Switching models should provide clear state feedback.

The user should never wonder whether the application has frozen because a model is loading or unloading.

Show meaningful states such as:

- Loading
- Unloading
- Preparing GPU
- Generating
- Ready
- Failed

Actual timing should be benchmarked.


6. EXPLICIT NON-GOALS

NOT A CLOUD AI PLATFORM

We are NOT building a SaaS platform requiring:

- Hosted inference
- Cloud databases
- Mandatory accounts
- Remote AI APIs

NOT A GENERIC AI AGENT WITH UNRESTRICTED COMPUTER CONTROL

The application should NOT become an AI that can freely:

- Execute shell commands
- Launch arbitrary programs
- Manipulate arbitrary files
- Control the operating system without explicit application-defined actions

AI actions must be typed and validated.

NOT A BROWSER-FIRST APPLICATION

The primary product is a desktop application.

A web UI could potentially exist in the future for other purposes, but the core product is desktop/local.

NOT A MULTI-USER SERVER

The initial product is designed around one local user and local application state.

We are not designing the core architecture around:

- User accounts
- Organizations
- Cloud synchronization
- Server-side tenancy
- Multi-user permissions

NOT A REPLACEMENT FOR EVERY AI FRAMEWORK

We do not need to implement our own:

- LLM runtime
- CUDA backend
- Image-generation engine
- STT engine
- TTS engine

The application should orchestrate proven local runtimes where appropriate.

NOT EVERYTHING AT ONCE

The final vision is large, but development must be incremental.

The first useful vertical slice should be approximately:

UI
↓
Tauri IPC
↓
Rust
↓
Conversation engine
↓
Local LLM
↓
Streaming response
↓
UI

Then capabilities should be added systematically.

NOT PERFECT IDENTITY GENERATION

We should aim for strong character consistency.

We should NOT promise mathematically perfect identity preservation across arbitrary generations.

The system should actively improve consistency and detect obvious failures.


7. CONTENT / MATURITY CONSIDERATIONS

The application is intended to be LOCAL-FIRST and USER-CONTROLLED.

The architecture should therefore NOT depend on a mandatory external moderation service.

Core conversations and generation should not need to be sent to a cloud moderation API.

There should be no assumption that every generation or conversation must be uploaded to a remote moderation provider.

No censoring should be in place. NSFW/Explicit content is allowed and will be up to user to moderate.

ARCHITECTURAL IMPLICATIONS

LOCAL PROCESSING

Content should remain local unless the user explicitly chooses an external feature.

NO MANDATORY MODERATION API

Core functionality should not depend on a remote moderation provider.

NO HIDDEN CLOUD PROCESSING

The application should not silently upload:

- Conversations
- Character data
- Memories
- Images
- Audio
- Prompts

to third-party services.

AI OUTPUT REMAINS UNTRUSTED

Regardless of content policy, generated output must never automatically become:

- Executable commands
- Shell commands
- Arbitrary filesystem operations
- Arbitrary process launches
- Database commands
- Unrestricted application actions

AI-triggered behavior must use explicitly defined, typed and validated application actions.


8. OVERALL PRODUCT EXPERIENCE

The long-term product should feel like:

"A private local AI environment where the user can build ongoing relationships with persistent AI characters, talk to them through text or voice, and experience them as persistent entities with memories, personalities and visual identities."

The most important word is:

PERSISTENT.

A conversation should not feel disposable.

A character should not feel disposable.

An image should not feel disconnected from the character who generated or sent it.

The application should progressively build a coherent local world of:

Characters
↓
Conversations
↓
Memories
↓
Relationships
↓
Images
↓
Visual identity

while remaining:

Local
Private
Offline-capable
Resource-aware
Reliable
Maintainable

This product definition is the starting point for formal requirements work.

It should NOT be treated as permission to skip architecture research or invent implementation details that have not yet been validated.