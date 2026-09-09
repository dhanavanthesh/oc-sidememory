use crate::primitives::TokenId;

use super::guide::GuideError;

pub(crate) fn required_words(model_width: usize) -> usize {
    model_width.div_ceil(32)
}

pub(crate) fn set_token(mask: &mut [u32], token: TokenId) -> Result<(), GuideError> {
    let token = usize::try_from(token).map_err(|_| GuideError::InternalInvariant {
        context: "token ID does not fit usize".into(),
    })?;
    let word = mask
        .get_mut(token / 32)
        .ok_or(GuideError::InternalInvariant {
            context: "allowed token exceeds model width".into(),
        })?;
    *word |= 1_u32 << (token % 32);
    Ok(())
}

pub(crate) fn clear_padding(mask: &mut [u32], model_width: usize) {
    let remainder = model_width % 32;
    if remainder != 0 {
        if let Some(last) = mask.last_mut() {
            *last &= (1_u32 << remainder) - 1;
        }
    }
}
