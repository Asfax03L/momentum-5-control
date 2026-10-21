//! Sorties audio de Windows, en lecture seule (WASAPI) : sortie par défaut,
//! sorties actives du casque, format de mixage, volume et muet.
//!
//! Rien n'est modifié : ni la sortie par défaut, ni le volume. La
//! classification (casque, Bluetooth/USB, mains libres) est faite par
//! `audio_check::classify`, testée sans Windows.

use momentum_core::audio_check::{AudioOutput, MixFormat, WindowsAudio, classify};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eMultimedia, eRender,
};
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    STGM_READ,
};
use windows::core::GUID;

const fn key(fmtid: u128, pid: u32) -> PROPERTYKEY {
    PROPERTYKEY {
        fmtid: GUID::from_u128(fmtid),
        pid,
    }
}

/// Nom complet de la sortie (`PKEY_Device_FriendlyName`).
const FRIENDLY_NAME: PROPERTYKEY = key(0xa45c254e_df1c_4efd_8020_67d146a850e0, 14);

/// Textes servant seulement à classer la sortie (jamais affichés : certains
/// contiennent l'adresse Bluetooth). Énumérateur (`PKEY_Device_EnumeratorName`)
/// et, relevés dans les propriétés des sorties du casque sur ce PC,
/// identifiants du périphérique et nom de l'appareil Bluetooth.
const CLASSIFY_KEYS: [PROPERTYKEY; 5] = [
    key(0xa45c254e_df1c_4efd_8020_67d146a850e0, 24),
    key(0xb3f8fa53_0004_438e_9003_51a46e139bfc, 2),
    key(0xb3f8fa53_0004_438e_9003_51a46e139bfc, 6),
    key(0xb3f8fa53_0004_438e_9003_51a46e139bfc, 26),
    key(0xb3f8fa53_0004_438e_9003_51a46e139bfc, 39),
];

/// Lit les sorties de Windows. `device_name` : nom du casque lu par le
/// pilotage, pour reconnaître une sortie renommée.
pub fn read(device_name: Option<&str>) -> Result<WindowsAudio, String> {
    // SAFETY: COM initialisé sur ce thread (sans effet s'il l'est déjà) ;
    // toutes les interfaces sont libérées à leur sortie de portée.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("énumération des sorties : {e}"))?;
        // Pas de sortie par défaut : erreur « élément introuvable ».
        let default_output = enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .ok()
            .map(|d| describe(&d, device_name));
        let mut headset_outputs = Vec::new();
        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("liste des sorties : {e}"))?;
        for i in 0..collection.GetCount().unwrap_or(0) {
            if let Ok(device) = collection.Item(i) {
                let output = describe(&device, device_name);
                if output.headset {
                    headset_outputs.push(output);
                }
            }
        }
        Ok(WindowsAudio {
            default_output,
            headset_outputs,
        })
    }
}

/// Une sortie : nom, classe, format de mixage, volume.
unsafe fn describe(device: &IMMDevice, device_name: Option<&str>) -> AudioOutput {
    // SAFETY: appels COM en lecture sur un périphérique valide.
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok();
        let text = |k: &PROPERTYKEY| store.as_ref().and_then(|s| property_text(s, k));
        let name = text(&FRIENDLY_NAME).unwrap_or_else(|| "sortie sans nom".to_string());
        let mut texts = vec![name.clone()];
        texts.extend(CLASSIFY_KEYS.iter().filter_map(text));
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let (headset, link, hands_free) = classify(&refs, device_name);
        let volume: Option<IAudioEndpointVolume> = device.Activate(CLSCTX_ALL, None).ok();
        AudioOutput {
            name,
            headset,
            link,
            hands_free,
            mix: mix_format(device),
            muted: volume
                .as_ref()
                .and_then(|v| v.GetMute().ok())
                .map(|b| b.as_bool()),
            volume: volume
                .as_ref()
                .and_then(|v| v.GetMasterVolumeLevelScalar().ok())
                .map(|v| (v.clamp(0.0, 1.0) * 100.0).round() as u8),
        }
    }
}

/// Format partagé du mixeur de Windows pour cette sortie.
unsafe fn mix_format(device: &IMMDevice) -> Option<MixFormat> {
    // SAFETY: le format rendu par GetMixFormat est alloué par COM et libéré
    // par CoTaskMemFree après lecture ; la partie « extensible » n'est lue
    // que si l'en-tête l'annonce avec la taille suffisante.
    unsafe {
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
        let format = client.GetMixFormat().ok()?;
        if format.is_null() {
            return None;
        }
        let header: WAVEFORMATEX = *format;
        // WAVE_FORMAT_EXTENSIBLE (0xFFFE) et WAVE_FORMAT_IEEE_FLOAT (3).
        let float = if header.wFormatTag == 0xFFFE
            && usize::from(header.cbSize) + size_of::<WAVEFORMATEX>()
                >= size_of::<WAVEFORMATEXTENSIBLE>()
        {
            let ext = *(format as *const WAVEFORMATEXTENSIBLE);
            let sub = ext.SubFormat;
            sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            header.wFormatTag == 3
        };
        let mix = MixFormat {
            channels: header.nChannels,
            rate_hz: header.nSamplesPerSec,
            bits: header.wBitsPerSample,
            float,
        };
        CoTaskMemFree(Some(format as *const _));
        Some(mix)
    }
}

/// Valeur texte d'une propriété, si elle existe.
unsafe fn property_text(
    store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore,
    key: &PROPERTYKEY,
) -> Option<String> {
    // SAFETY: la valeur est convertie puis libérée (PropVariantClear) ; la
    // chaîne allouée par PropVariantToStringAlloc l'est par CoTaskMemFree.
    unsafe {
        let mut value = store.GetValue(key).ok()?;
        let text = PropVariantToStringAlloc(&value).ok();
        let _ = PropVariantClear(&mut value);
        let text = text?;
        let out = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        out.filter(|t| !t.is_empty())
    }
}

#[cfg(test)]
mod tests {
    /// Lecture réelle des sorties de ce PC (lecture seule) ; lancée à la
    /// main : `cargo test -p momentum-service audio_output -- --ignored --nocapture`.
    #[test]
    #[ignore = "dépend des sorties audio du PC"]
    fn read_this_pc() {
        let audio = super::read(None).unwrap();
        println!("{audio:#?}");
    }
}
