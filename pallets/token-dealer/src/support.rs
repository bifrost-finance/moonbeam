// Copyright 2019-2020 PureStake Inc.
// This file is part of Moonbeam.

// Moonbeam is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Moonbeam is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Moonbeam.  If not, see <http://www.gnu.org/licenses/>.

//! XCM objects and relationships
use frame_support::{
	debug,
	traits::{Currency, ExistenceRequirement, Get, WithdrawReasons},
};
use sp_runtime::traits::{CheckedConversion, Convert};
use sp_std::{
	convert::{TryFrom, TryInto},
	marker::PhantomData,
	prelude::*,
	result,
};
use token_factory::{CurrencyId, Ticker};
use xcm::v0::{Error, Junction, MultiAsset, MultiLocation, Result as XcmResult};
use xcm_executor::traits::{LocationConversion, MatchesFungible, TransactAsset};

pub trait CurrencyIdConversion<CurrencyId> {
	fn from_asset(asset: &MultiAsset) -> Option<CurrencyId>;
}

/// The handler for processing cross-chain messages
pub struct MultiCurrencyAdapter<
	NativeCurrency,
	TokenFactory,
	Matcher,
	AccountIdConverter,
	AccountId,
	CurrencyIdConverter,
	CurrencyId,
>(
	PhantomData<(
		NativeCurrency,
		TokenFactory,
		Matcher,
		AccountIdConverter,
		AccountId,
		CurrencyIdConverter,
		CurrencyId,
	)>,
);

impl<
		NativeCurrency: Currency<AccountId>,
		TokenFactory: token_factory::TokenMinter<Ticker, AccountId, NativeCurrency::Balance>,
		Matcher: MatchesFungible<NativeCurrency::Balance>,
		AccountIdConverter: LocationConversion<AccountId>,
		AccountId: sp_std::fmt::Debug + Clone,
		CurrencyIdConverter: CurrencyIdConversion<CurrencyId>,
	> TransactAsset
	for MultiCurrencyAdapter<
		NativeCurrency,
		TokenFactory,
		Matcher,
		AccountIdConverter,
		AccountId,
		CurrencyIdConverter,
		CurrencyId,
	>
{
	fn deposit_asset(asset: &MultiAsset, location: &MultiLocation) -> XcmResult {
		debug::info!("------------------------------------------------");
		debug::info!(
			">>> trying deposit. asset: {:?}, location: {:?}",
			asset,
			location
		);
		let who = AccountIdConverter::from_location(location).ok_or(())?;
		debug::info!("who: {:?}", who);
		let currency = CurrencyIdConverter::from_asset(asset).ok_or(())?;
		debug::info!("currency_id: {:?}", currency);
		let amount: NativeCurrency::Balance = Matcher::matches_fungible(&asset).ok_or(())?;
		debug::info!("amount: {:?}", amount);
		// match on currency variant
		if let CurrencyId::Token(token_id) = currency {
			// mint erc20 token to `who`
			TokenFactory::mint(token_id, who.clone(), amount).map_err(|error| {
				debug::info!(
					"Token factory `mint` failed
					\n token_id: {:?}\n who: {:?}\n amount: {:?}\n error: {:?}",
					token_id,
					who,
					amount,
					error
				);
				()
			})?;
		} else {
			// native currency transfer via `frame/pallet_balances` is only other variant
			NativeCurrency::deposit_creating(&who, amount);
		}
		debug::info!(">>> successful deposit.");
		debug::info!("------------------------------------------------");
		Ok(())
	}

	fn withdraw_asset(
		asset: &MultiAsset,
		location: &MultiLocation,
	) -> result::Result<MultiAsset, Error> {
		debug::info!("------------------------------------------------");
		debug::info!(
			">>> trying withdraw. asset: {:?}, location: {:?}",
			asset,
			location
		);
		let who = AccountIdConverter::from_location(location).ok_or(())?;
		debug::info!("who: {:?}", who);
		let currency = CurrencyIdConverter::from_asset(asset).ok_or(())?;
		debug::info!("currency_id: {:?}", currency);
		let amount: NativeCurrency::Balance = Matcher::matches_fungible(&asset).ok_or(())?;
		debug::info!("amount: {:?}", amount);
		// match on currency variant
		if let CurrencyId::Token(token_id) = currency {
			// burn erc20 token from `who`
			TokenFactory::burn(token_id, who.clone(), amount).map_err(|error| {
				debug::info!(
					"Token factory `burn` failed
					\n token_id: {:?}\n who: {:?}\n amount: {:?}\n error: {:?}",
					token_id,
					who,
					amount,
					error
				);
				()
			})?;
		} else {
			// native currency transfer via `frame/pallet_balances` is only other variant
			NativeCurrency::withdraw(
				&who,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::AllowDeath,
			)
			.map_err(|error| {
				debug::info!(
					"Native currency `withdraw` failed\n who: {:?}\n amount: {:?}\n error: {:?}",
					who,
					amount,
					error
				);
				()
			})?;
		}
		debug::info!(">>> successful withdraw.");
		debug::info!("------------------------------------------------");
		Ok(asset.clone())
	}
}

/// Matcher associated type for MultiCurrencyAdapter to convert assets into local types
pub struct IsConcreteWithGeneralKey<CurrencyId, FromRelayChainBalance>(
	PhantomData<(CurrencyId, FromRelayChainBalance)>,
);
impl<CurrencyId, B, FromRelayChainBalance> MatchesFungible<B>
	for IsConcreteWithGeneralKey<CurrencyId, FromRelayChainBalance>
where
	CurrencyId: TryFrom<Vec<u8>>,
	B: TryFrom<u128>,
	FromRelayChainBalance: Convert<u128, u128>,
{
	fn matches_fungible(a: &MultiAsset) -> Option<B> {
		if let MultiAsset::ConcreteFungible { id, amount } = a {
			if id == &MultiLocation::X1(Junction::Parent) {
				// Convert relay chain decimals to local chain
				let local_amount = FromRelayChainBalance::convert(*amount);
				return CheckedConversion::checked_from(local_amount);
			}
			if let Some(Junction::GeneralKey(key)) = id.last() {
				if TryInto::<CurrencyId>::try_into(key.clone()).is_ok() {
					return CheckedConversion::checked_from(*amount);
				}
			}
		}
		None
	}
}

/// Converter from MultiAsset to local Currency type
pub struct CurrencyIdConverter<CurrencyId, RelayChainCurrencyId>(
	PhantomData<CurrencyId>,
	PhantomData<RelayChainCurrencyId>,
);
impl<CurrencyId, RelayChainCurrencyId> CurrencyIdConversion<CurrencyId>
	for CurrencyIdConverter<CurrencyId, RelayChainCurrencyId>
where
	CurrencyId: TryFrom<Vec<u8>>,
	RelayChainCurrencyId: Get<CurrencyId>,
{
	fn from_asset(asset: &MultiAsset) -> Option<CurrencyId> {
		if let MultiAsset::ConcreteFungible { id: location, .. } = asset {
			if location == &MultiLocation::X1(Junction::Parent) {
				return Some(RelayChainCurrencyId::get());
			}
			if let Some(Junction::GeneralKey(key)) = location.last() {
				return CurrencyId::try_from(key.clone()).ok();
			}
		}
		None
	}
}
