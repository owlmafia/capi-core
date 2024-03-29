/// Some quick tests to confirm not documented Algorand functionality
/// [ignore] because we don't test Algorand here, this is so to say a documentation substitute.
#[cfg(test)]
#[allow(dead_code)]
pub mod test {
    use crate::algo_helpers::{send_tx_and_wait, send_txs_and_wait};
    use algonaut::{
        algod::v2::Algod,
        core::{Address, MicroAlgos},
        transaction::{
            account::Account, contract_account::ContractAccount, transaction::StateSchema,
            tx_group::TxGroup, AcceptAsset, CreateApplication, CreateAsset, Pay, Transaction,
            TransferAsset, TxnBuilder,
        },
    };
    use anyhow::Result;
    use mbase::{
        dependencies::algod_for_tests, teal::load_teal,
        util::network_util::wait_for_pending_transaction,
    };
    use network_test_util::{
        test_data::{creator, investor1},
        test_init,
    };
    use tokio::test;

    pub async fn create_always_approves_app(
        algod: &Algod,
        sender: &Address,
    ) -> Result<Transaction> {
        let always_succeeds_source = load_teal("always_succeeds").unwrap();

        let compiled_approval_program = algod.compile_teal(&always_succeeds_source.0).await?;
        let compiled_clear_program = algod.compile_teal(&always_succeeds_source.0).await?;

        let params = algod.suggested_transaction_params().await?;
        Ok(TxnBuilder::with(
            &params,
            CreateApplication::new(
                *sender,
                compiled_approval_program.clone(),
                compiled_clear_program,
                StateSchema {
                    number_ints: 0,
                    number_byteslices: 0,
                },
                StateSchema {
                    number_ints: 0,
                    number_byteslices: 0,
                },
            )
            .build(),
        )
        .build()?)
    }

    pub async fn pay(algod: &Algod, sender: &Address) -> Result<Transaction> {
        let params = algod.suggested_transaction_params().await?;
        // sender sends a payment to themselves - don't need another party right now
        Ok(TxnBuilder::with(
            &params,
            Pay::new(*sender, *sender, MicroAlgos(10_000)).build(),
        )
        .build()?)
    }

    pub async fn pay_and_submit(algod: &Algod, sender: &Account) -> Result<()> {
        let tx = pay(algod, &sender.address()).await?;
        let signed = sender.sign_transaction(tx)?;
        send_tx_and_wait(algod, &signed).await?;
        Ok(())
    }

    pub async fn optin_to_asset(
        algod: &Algod,
        sender: &Address,
        asset_id: u64,
    ) -> Result<Transaction> {
        let params = algod.suggested_transaction_params().await?;
        Ok(TxnBuilder::with(&params, AcceptAsset::new(*sender, asset_id).build()).build()?)
    }

    pub async fn create_asset_tx(algod: &Algod, sender: &Address) -> Result<Transaction> {
        let params = algod.suggested_transaction_params().await?;
        Ok(TxnBuilder::with(
            &params,
            CreateAsset::new(*sender, 1000, 0, false)
                .unit_name("FOO".to_owned())
                .asset_name("foo".to_owned())
                .build(),
        )
        .build()?)
    }

    pub async fn transfer_asset_tx(
        algod: &Algod,
        sender: &Address,
        receiver: &Address,
        asset_id: u64,
        amount: u64,
    ) -> Result<Transaction> {
        let params = algod.suggested_transaction_params().await?;
        Ok(TxnBuilder::with(
            &params,
            TransferAsset::new(*sender, asset_id, amount, *receiver).build(),
        )
        .build()?)
    }

    pub async fn create_asset_and_sign(algod: &Algod, sender: &Account) -> Result<u64> {
        let create_asset_tx = create_asset_tx(&algod, &sender.address()).await?;
        let create_asset_signed_tx = sender.sign_transaction(create_asset_tx)?;
        let p_tx = send_tx_and_wait(algod, &create_asset_signed_tx).await?;
        let asset_id = p_tx.asset_index.unwrap();
        Ok(asset_id)
    }

    pub async fn transfer_asset_and_sign(
        algod: &Algod,
        sender: &Account,
        receiver: &Address,
        asset_id: u64,
        amount: u64,
    ) -> Result<()> {
        let transfer_tx =
            transfer_asset_tx(&algod, &sender.address(), receiver, asset_id, amount).await?;
        let transfer_signed_tx = sender.sign_transaction(transfer_tx)?;
        let transfer_res = algod
            .broadcast_signed_transaction(&transfer_signed_tx)
            .await?;
        wait_for_pending_transaction(&algod, &transfer_res.tx_id.parse()?).await?;
        Ok(())
    }

    #[test]
    #[ignore]
    async fn create_app_has_to_be_first_in_group_to_retrieve_app_id() -> Result<()> {
        let algod = algod_for_tests();

        let sender = creator();

        let mut create_app_tx = create_always_approves_app(&algod, &sender.address()).await?;

        let mut pay_tx = pay(&algod, &sender.address()).await?;

        TxGroup::assign_group_id(&mut [&mut create_app_tx, &mut pay_tx]).unwrap();

        let create_app_signed_tx = sender.sign_transaction(create_app_tx)?;
        let pay_signed_tx = sender.sign_transaction(pay_tx)?;

        let p_tx = send_txs_and_wait(&algod, &[create_app_signed_tx, pay_signed_tx]).await?;
        let app_id = p_tx.application_index;
        log::debug!("app_id: {:?}", app_id);

        Ok(())
    }

    #[test]
    #[ignore]
    async fn cannot_create_asset_and_app_in_same_group() -> Result<()> {
        test_init().await?;

        let algod = algod_for_tests();
        let creator = creator();

        let mut create_app_tx = create_always_approves_app(&algod, &creator.address()).await?;
        let mut create_asset_tx = create_asset_tx(&algod, &creator.address()).await?;

        TxGroup::assign_group_id(&mut [&mut create_app_tx, &mut create_asset_tx])?;

        let create_app_signed_tx_signed = creator.sign_transaction(create_app_tx)?;
        let create_asset_signed_tx_signed = creator.sign_transaction(create_asset_tx)?;

        let res = algod
            .broadcast_signed_transactions(&[
                create_app_signed_tx_signed,
                create_asset_signed_tx_signed,
            ])
            .await
            .unwrap();

        let p_tx = wait_for_pending_transaction(&algod, &res.tx_id.parse()?)
            .await
            .unwrap()
            .unwrap();
        println!("{p_tx:?}");

        // Only the asset/app id for the first tx in the group is set
        assert!(p_tx.application_index.is_some());
        assert!(p_tx.asset_index.is_none());

        Ok(())
    }

    #[test]
    #[ignore]
    async fn optin_and_receive_asset_can_be_in_the_same_group() -> Result<()> {
        let algod = algod_for_tests();

        let asset_creator_and_sender = creator();
        let assset_receiver = investor1();

        let asset_id = create_asset_and_sign(&algod, &asset_creator_and_sender).await?;

        let mut optin_to_asset_tx =
            optin_to_asset(&algod, &assset_receiver.address(), asset_id).await?;
        let mut receive_asset_tx = transfer_asset_tx(
            &algod,
            &asset_creator_and_sender.address(),
            &assset_receiver.address(),
            asset_id,
            100,
        )
        .await?;

        TxGroup::assign_group_id(&mut [&mut optin_to_asset_tx, &mut receive_asset_tx]).unwrap();

        // asset receiver signs their optin
        let optin_to_asset_signed_tx = assset_receiver.sign_transaction(optin_to_asset_tx)?;
        // asset creator/sender signs sending the asset to the receiver
        let receive_asset_signed_tx =
            asset_creator_and_sender.sign_transaction(receive_asset_tx)?;

        let res = algod
            .broadcast_signed_transactions(&[optin_to_asset_signed_tx, receive_asset_signed_tx])
            .await;

        log::debug!("res: {:?}", res);
        assert!(res.is_ok());

        Ok(())
    }

    #[test]
    #[ignore]
    async fn rekey_to_lsig_works() -> Result<()> {
        let algod = algod_for_tests();

        // regular account rekeys (this can be the address which with we receive the fee)
        let rekeyed_acc = creator();
        let rekeyed_acc_address = rekeyed_acc.address();

        // rekeyed to lsig: so anyone can withdraw from the address, as long as it's allowed by the program
        // this also invalidates the old account as sender
        let program = algod
            .compile_teal(
                r#"
    #pragma version 6
    int 1
    "#
                .as_bytes(),
            )
            .await?;
        let rekey_to_acc = ContractAccount::new(program);
        let rekey_to_acc_address = rekey_to_acc.address().to_owned();
        println!("rekey_to_acc_address: {rekey_to_acc_address:?}");

        // rekey
        let params = algod.suggested_transaction_params().await?;
        let rekey_tx = TxnBuilder::with(
            &params,
            Pay::new(rekeyed_acc_address, rekeyed_acc_address, MicroAlgos(0)).build(),
        )
        .rekey_to(rekey_to_acc_address)
        .build()?;
        let rekey_signed = rekeyed_acc.sign_transaction(rekey_tx)?;
        let rekey_response = algod.broadcast_signed_transaction(&rekey_signed).await?;
        wait_for_pending_transaction(&algod, &rekey_response.tx_id.parse()?).await?;
        println!("Rekey success");

        // verify: rekey_to address is set as auth address of the rekeyed acc
        let account_infos = algod.account_information(&rekeyed_acc_address).await?;
        assert_eq!(Some(rekey_to_acc_address), account_infos.auth_addr);

        // verify: send a tx with the rekeyed address as sender, signing with rekey_to account
        // unwrap: this is a test
        let receiver = "PGCS3D5JL4AIFGTBPDGGMMCT3ODKUUFEFG336MJO25CGBG7ORKVOE3AHSU"
            .parse()
            .unwrap();
        let payment_tx = TxnBuilder::with(
            &params,
            Pay::new(rekeyed_acc_address, receiver, MicroAlgos(10_000)).build(),
        )
        .build()?;
        let payment_signed = rekey_to_acc.sign(payment_tx, vec![])?;
        let payment_response = algod.broadcast_signed_transaction(&payment_signed).await;
        println!("Payment response: {:?}", payment_response);
        assert!(payment_response.is_ok());

        // verify: the original account can't send anymore
        // note: different amount so tx has different id (preventx "tx already in ledger")
        let new_payment_tx = TxnBuilder::with(
            &params,
            Pay::new(rekeyed_acc_address, receiver, MicroAlgos(11_000)).build(),
        )
        .build()?;
        let new_payment_signed = rekeyed_acc.sign_transaction(new_payment_tx)?;
        let new_payment_response = algod
            .broadcast_signed_transaction(&new_payment_signed)
            .await;
        // we get the expected error: complains about the original account authorizing the tx, instead of the program:
        // "should have been authorized by ZG2RRCHBZ4K2QKP3NGMYVF2MVG7YW2TSNJPVFVLEGX7KGQ46QVPJGOFTK4 but was actually authorized by STOUDMINSIPP7JMJMGXVJYVS6HHD3TT5UODCDPYGV6KBGP7UYNTLJVJJME""
        println!("New payment response: {:?}", new_payment_response);
        assert!(new_payment_response.is_err());

        Ok(())
    }
}
