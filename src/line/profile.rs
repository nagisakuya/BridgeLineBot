use super::*;

#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
pub struct UserProfile {
    pub displayName: String,
    pub userId: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub pictureUrl: Option<String>,
    #[serde(default)]
    pub statusMessage: Option<String>,
}
pub async fn get_user_profile_from_friend(user_id: String) -> Option<UserProfile> {
    let resp = send_get_request(&format!("https://api.line.me/v2/bot/profile/{user_id}"))
        .await
        .unwrap();
    if resp.status() != 200 {
        return None;
    }
    let profile: UserProfile = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    Some(profile)
}

pub async fn get_user_profile_from_group(user_id: &str, group_id: &str) -> Option<UserProfile> {
    let resp = send_get_request(&format!(
        "https://api.line.me/v2/bot/group/{group_id}/member/{user_id}"
    ))
    .await
    .unwrap();
    if resp.status() != 200 {
        return None;
    }
    let profile: UserProfile = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    Some(profile)
}